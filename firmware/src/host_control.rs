//! Firmware side of the host control channel, shared by `main` and `podtest` through the package
//! library. The wire grammar (`CTL VER|RSN|RST|SET|REL`, `CTLR` replies) is `asperitas_wire::control`;
//! the knob table is `asperitas_pod::knob_override`.
//!
//! Three pieces of state live here, each owned by the thing that cannot be rebuilt elsewhere:
//!
//! - [`OVERRIDE`], the knob-boundary override table (`asperitas_pod::knob_override`), written by
//!   the control task and read by whoever turns knob readings into DSP parameters via [`merge`].
//! - the reset reason, captured by [`capture_reset_reason`] before anything clears `RCC_RSR`.
//! - the version string baked in by `build.rs`.
//!
//! Replies and restart never wait on anything the host or the audio path controls. Replies use
//! `emit_record`, which commits whole or not at all and returns `false` when the console has no
//! room; a lost reply is the host's timeout to handle, a blocked task would be the board's problem.
//!
//! # Wiring
//!
//! Console builds only (`log-usb`); the RTT-only build has no inbound path and compiles this module
//! down to [`capture_reset_reason`] and [`merge`].
//!
//! - **Consumer.** `spawn` calls `usb::enable_inbound()` before `Console::run()` starts and spawns
//!   one executor task that `receive()`s `InboundRecord`s and hands each to `control::route`. The
//!   task does nothing slow, so the 4-deep inbound channel does not stay full and the USB reader's
//!   flow control does not engage in practice.
//! - **Replies never block.** They go through `emit_record`, which commits whole or not at all and
//!   returns `false` when the console has no room. A dropped reply is the host's timeout to handle.
//! - **Override.** `SET`/`REL` write a `cortex_m::interrupt::Mutex<RefCell<KnobOverride<2>>>`.
//!   `main` calls [`merge`] on the knob readings inside the audio callback, after the pot read and
//!   before `params_from_normalised`, so injected values pass through the DSP smoothing as a turned
//!   pot does. Knob ids 0 and 1; anything else is `bad-knob`. `podtest` logs raw ADC counts to
//!   measure the pot itself, so it accepts and holds overrides but does not merge them into its log.
//! - **Version.** `build.rs` exports `ASPERITAS_VERSION` = `CARGO_PKG_VERSION` + `+` +
//!   `git describe --always --dirty --tags` (the package version alone without git), filtered to
//!   printable ASCII and cut to 32 bytes. `-dirty` can lag: only HEAD moves trigger a rerun.
//! - **Reset reason.** [`capture_reset_reason`] is the first statement of `main`: it reads `RCC_RSR`
//!   into a static, then sets `RMVF` to clear it (the flags are sticky across resets). The mapping
//!   is `control::reset_reason_from_rsr`, host-tested; priority is specific cause first, because a
//!   power-on also sets BOR and PIN, and a software reset also sets PIN. A bootloader that cleared
//!   the register before our code ran reads `unknown`.
//! - **Restart.** `RST` replies `OK RST`, waits 20 ms on the control task's own timer for the
//!   console drain to ship it, then `SCB::sys_reset()`. It depends on neither the audio path nor
//!   channel space (only on the executor running), masks nothing, and leaves BOOT0 and NRST alone,
//!   so the BOOT/RESET/DFU sequence works exactly as before. A reset lands in the application, not
//!   the ROM bootloader.
//! - **Panic path.** The panic handler takes no lock and touches no inbound state, so inbound
//!   traffic cannot block it. After a panic the executor is dead, so `RST` is not serviced; recover
//!   with the RESET button.
//! - **Rig.** The rig owns its inbound channel (one consumer shared with the excerpt installer), so
//!   it calls [`handle_control`] from its install loop instead of [`spawn`], passing a hook for
//!   `FX`. Overrides, `VER`, `RSN` and `RST` are the same code as in `main`.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use asperitas_pod::knob_override::KnobOverride;
#[cfg(feature = "log-usb")]
use asperitas_wire::control::{VersionText, VERSION_MAX};
use cortex_m::interrupt::Mutex;

/// Knobs the Pod exposes to the DSP: knob 1 and knob 2, ids 0 and 1.
pub const KNOBS: usize = asperitas_dsp::KNOB_COUNT;

static OVERRIDE: Mutex<RefCell<KnobOverride<KNOBS>>> =
    Mutex::new(RefCell::new(KnobOverride::new()));

/// `RCC_RSR` as it read at boot, before [`capture_reset_reason`] cleared it.
static RESET_STATUS: AtomicU32 = AtomicU32::new(0);

/// Read `RCC_RSR` and then clear it. Call first thing in `main`, before anything else could
/// consume the flags.
///
/// The clear matters: the flags are sticky across resets, so a second reset would otherwise report
/// the union of both causes, including a stale power-on.
pub fn capture_reset_reason() {
    let rcc = daisy_embassy::hal::pac::RCC;
    RESET_STATUS.store(rcc.rsr().read().0, Ordering::Relaxed);
    rcc.rsr().modify(|w| w.set_rmvf(true));
}

/// Resolve the value of each knob: the host's where overridden, the pot's otherwise. Cheap enough
/// for the audio callback: one critical section around a two-element copy.
pub fn merge(knobs: [f32; KNOBS]) -> [f32; KNOBS] {
    cortex_m::interrupt::free(|cs| OVERRIDE.borrow(cs).borrow().merge(knobs))
}

#[cfg(feature = "log-usb")]
fn version() -> VersionText {
    let v = env!("ASPERITAS_VERSION").as_bytes();
    VersionText::version(&v[..v.len().min(VERSION_MAX)])
        .unwrap_or_else(|_| VersionText::version(b"unknown").unwrap())
}

#[cfg(feature = "log-usb")]
mod console {
    use super::*;
    use asperitas_logging::inbound::InboundRecord;
    use asperitas_logging::usb::{enable_inbound, InboundReceiver};
    use asperitas_wire::control::{
        reset_reason_from_rsr, route, wire_to_level, Command, ErrorCode, Reply, Verb, CONTROL,
    };

    /// Time the console drain gets to ship the `OK RST` ack before the core resets. The drain is a
    /// separate task and a USB bulk packet goes out within a millisecond or two.
    const RESTART_FLUSH_MS: u64 = 20;

    fn send(reply: Reply) {
        let mut body = [0u8; asperitas_wire::frame::MAX_BODY];
        let n = reply.encode(&mut body);
        let now_ms = embassy_time::Instant::now().as_millis() as u32;
        // Best effort by design; see the module docs.
        let _ = asperitas_logging::emit_record(asperitas_logging::Level::Info, now_ms, &body[..n]);
    }

    /// Answer one `CTL` record: the verbs every binary serves (`VER`, `RSN`, `RST`, `SET`, `REL`)
    /// plus whatever `replay_fx` makes of `FX`, then send the reply.
    ///
    /// `replay_fx(fx, value)` is the binary-specific hook: `main` and `podtest` have no effect
    /// stage and answer `unknown-verb`; the rig selects its replay effect. `RST` acks, waits for the
    /// console drain, and resets the core, so this future does not return for it.
    pub async fn handle_control(rec: &InboundRecord, replay_fx: impl FnOnce(u8, u16) -> Reply) {
        let mut restart = false;
        let mut reply = None;
        let rejected = route(
            rec,
            &CONTROL,
            // Excerpt records belong to the binary's own consumer, not to this dispatcher.
            |_| {},
            |cmd| match cmd {
                Command::Restart => restart = true,
                other => reply = Some(apply(other, replay_fx)),
            },
        );
        if restart {
            send(Reply::Ok(Verb::Restart));
            embassy_time::Timer::after_millis(RESTART_FLUSH_MS).await;
            cortex_m::peripheral::SCB::sys_reset();
        }
        if let Some(r) = reply.or(rejected) {
            send(r);
        }
    }

    fn apply(cmd: Command, replay_fx: impl FnOnce(u8, u16) -> Reply) -> Reply {
        let ok = |cmd: Command| Reply::Ok(cmd.verb());
        match cmd {
            Command::GetVersion => Reply::Version(version()),
            Command::GetResetReason => {
                Reply::ResetReason(reset_reason_from_rsr(RESET_STATUS.load(Ordering::Relaxed)))
            }
            Command::OverrideSet { knob, value } => {
                let v = wire_to_level(value);
                let accepted = cortex_m::interrupt::free(|cs| {
                    OVERRIDE.borrow(cs).borrow_mut().set(usize::from(knob), v)
                });
                if accepted {
                    ok(cmd)
                } else {
                    Reply::Error(ErrorCode::BadKnob)
                }
            }
            Command::OverrideRelease { knob } => {
                if usize::from(knob) >= KNOBS {
                    return Reply::Error(ErrorCode::BadKnob);
                }
                cortex_m::interrupt::free(|cs| {
                    OVERRIDE.borrow(cs).borrow_mut().release(usize::from(knob))
                });
                ok(cmd)
            }
            Command::ReplayFx { fx, value } => replay_fx(fx, value),
            // Handled by `handle_control`: it never returns.
            Command::Restart => Reply::Error(ErrorCode::UnknownVerb),
        }
    }

    #[embassy_executor::task]
    async fn control_task(rx: InboundReceiver) {
        loop {
            let rec = rx.receive().await;
            // No effect stage in these binaries.
            handle_control(&rec, |_, _| Reply::Error(ErrorCode::UnknownVerb)).await;
        }
    }

    /// Enable the inbound console reader and start servicing control records. Call before
    /// the console starts running.
    pub fn spawn(spawner: &embassy_executor::Spawner) {
        let rx = enable_inbound();
        spawner.spawn(control_task(rx).expect("failed to spawn control task"));
    }
}

#[cfg(feature = "log-usb")]
pub use console::{handle_control, spawn};

/// No console, no control channel.
#[cfg(not(feature = "log-usb"))]
pub fn spawn(_spawner: &embassy_executor::Spawner) {}
