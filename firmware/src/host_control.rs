//! Firmware side of the host control channel (TASK-032.03).
//!
//! Shared by `main` and `podtest` with `#[path = "../host_control.rs"] mod host_control;`: the
//! package has no library target, and `src/bin/*.rs` files are each their own crate. Design and
//! rationale: "Host control channel" in `docs/reference/daisy-seed3.md`.
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

// `main` uses `merge` but `podtest` does not, and without the console nothing reads the reason or
// version; one shared file, several consumers, so unused-item lints are expected per binary.
#![allow(dead_code)]

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use asperitas_logging::control::{VersionText, VERSION_MAX};
use asperitas_pod::knob_override::KnobOverride;
use cortex_m::interrupt::Mutex;

/// Knobs the Pod exposes to the DSP: knob 1 and knob 2, ids 0 and 1.
pub const KNOBS: usize = 2;

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

fn version() -> VersionText {
    let v = env!("ASPERITAS_VERSION").as_bytes();
    VersionText::version(&v[..v.len().min(VERSION_MAX)])
        .unwrap_or_else(|_| VersionText::version(b"unknown").unwrap())
}

#[cfg(feature = "log-usb")]
mod console {
    use super::*;
    use asperitas_logging::control::{
        reset_reason_from_rsr, route, Command, ErrorCode, Reply, CONTROL,
    };
    use asperitas_logging::usb::{enable_inbound, InboundReceiver};

    /// Time the console drain gets to ship the `OK RST` ack before the core resets. The drain is a
    /// separate task and a USB bulk packet goes out within a millisecond or two.
    const RESTART_FLUSH_MS: u64 = 20;

    fn send(reply: Reply) {
        let mut body = [0u8; asperitas_logging::frame::MAX_BODY];
        let n = reply.encode(&mut body);
        let now_ms = embassy_time::Instant::now().as_millis() as u32;
        // Best effort by design; see the module docs.
        let _ = asperitas_logging::emit_record(asperitas_logging::Level::Info, now_ms, &body[..n]);
    }

    fn apply(cmd: Command) -> Option<Reply> {
        let ok = |cmd: Command| Some(Reply::Ok(cmd.verb()));
        match cmd {
            Command::GetVersion => Some(Reply::Version(version())),
            Command::GetResetReason => Some(Reply::ResetReason(reset_reason_from_rsr(
                RESET_STATUS.load(Ordering::Relaxed),
            ))),
            Command::OverrideSet { knob, value } => {
                let v = f32::from(value) / f32::from(u16::MAX);
                let accepted = cortex_m::interrupt::free(|cs| {
                    OVERRIDE.borrow(cs).borrow_mut().set(usize::from(knob), v)
                });
                if accepted {
                    ok(cmd)
                } else {
                    Some(Reply::Error(ErrorCode::BadKnob))
                }
            }
            Command::OverrideRelease { knob } => {
                if usize::from(knob) >= KNOBS {
                    return Some(Reply::Error(ErrorCode::BadKnob));
                }
                cortex_m::interrupt::free(|cs| {
                    OVERRIDE.borrow(cs).borrow_mut().release(usize::from(knob))
                });
                ok(cmd)
            }
            // Handled by the caller: it never returns.
            Command::Restart => None,
        }
    }

    #[embassy_executor::task]
    async fn control_task(rx: InboundReceiver) {
        loop {
            let rec = rx.receive().await;
            let mut restart = false;
            let mut reply = None;
            let rejected = route(
                &rec,
                &CONTROL,
                // No excerpt consumer in these binaries; the record is simply not for us.
                |_| {},
                |cmd| match cmd {
                    Command::Restart => restart = true,
                    other => reply = apply(other),
                },
            );
            if restart {
                send(Reply::Ok(asperitas_logging::control::Verb::Restart));
                embassy_time::Timer::after_millis(RESTART_FLUSH_MS).await;
                cortex_m::peripheral::SCB::sys_reset();
            }
            if let Some(r) = reply.or(rejected) {
                send(r);
            }
        }
    }

    /// Enable the inbound console reader and start servicing control records. Call before
    /// `usb::run()` starts.
    pub fn spawn(spawner: &embassy_executor::Spawner) {
        let rx = enable_inbound();
        spawner.spawn(control_task(rx).expect("failed to spawn control task"));
    }
}

#[cfg(feature = "log-usb")]
pub use console::spawn;
