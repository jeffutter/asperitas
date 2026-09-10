//! defmt transport: one `log::Record` in, exactly one defmt frame out.
//!
//! defmt shares nothing with [`crate::usb`]: no endpoint, no pipe, no drain task, no host-side
//! reader that can fall behind. It writes into a static ring in RAM that a debug probe reads,
//! which is also why it is the only channel able to speak during the pre-USB window where boot
//! faults live.
//!
//! # Why a bridge at all
//!
//! Call-sites use `log::info!`; defmt macros need a **literal** format string, so there is no
//! runtime-level defmt call to forward to. Hence the `match` in [`emit_frame`] — forced by the
//! macro, not stylistic. Everything else about the record is rendered once, by the same
//! [`crate::format_body`] the console uses, and shipped as a single `{}` argument.
//!
//! Two alternatives were considered and rejected:
//!
//! - `log-to-defmt` 0.1.0 — unpublished since 2023, and its own docs admit it maps every level
//!   to the most verbose defmt level, discarding the information this crate exists to carry.
//! - Converting the firmware call-sites to `defmt::info!` — abandons the facade whose whole
//!   purpose was to make "the probe arrived" a feature flag rather than a refactor.
//!
//! # What this costs, and where it hurts
//!
//! Every defmt frame is written inside `critical_section::acquire()` (defmt-rtt
//! src/lib.rs:168) and reentrancy is fatal (`panic!("defmt logger taken reentrantly")`,
//! src/lib.rs:173). With interrupts disabled against a ~667 µs audio block deadline, that makes
//! one rule absolute: **nothing may log from the audio callback.** This bridge upholds it by
//! never calling `log::` itself, so it cannot recurse into the logger it is serving.
//!
//! Whether a frame survives is conditional on the host. defmt-rtt initialises its up-channel in
//! `MODE_NON_BLOCKING_TRIM` (src/lib.rs:113), so unattached the records are truncated or
//! silently dropped; probe-rs flips the channel to block-if-full when it attaches, at which
//! point a stalled host means the target *spins* inside the write loop with IRQs off. Both
//! behaviours are defmt-rtt's, not ours, and both are visible the moment a probe is attached.
//!
//! # Filtering
//!
//! **An RTT build with `DEFMT_LOG` unset carries ERROR frames only.** When the variable is absent
//! entirely, defmt-macros uses `LEVEL_WHEN_NOTHING_IS_SPECIFIED = Some(Level::Error)`
//! (defmt-macros-1.1.1 src/function_like/log/env_filter.rs:34), so four of this module's five arms
//! expand to nothing and only `log::error!` survives. Measured on `main`: an unset build and a
//! `DEFMT_LOG=error` build are the same size exactly (48084 bytes), while `warn` is 48136, `info`
//! 48304, `debug` 48984, `trace` 49712 and `off` 44592.
//!
//! That is easy to mistake for a dead channel: `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1`
//! then attaching a probe shows a boot that says nothing, because `info!("Booting...")` was never
//! in the image. The facade's own runtime `set_max_level(LevelFilter::Info)` cannot help — it hands
//! the record here happily, and the arm that would have shipped it does not exist. Ask for what you
//! want at build time: `DEFMT_LOG=info make build FEATURES="seed3 log-defmt" NO_DEFAULT=1`.
//!
//! `DEFMT_LOG` is otherwise a *compile-time* filter, and it does reach this bridge: rebuilding the RTT image
//! with `DEFMT_LOG=off` removes [`emit_frame`] from the symbol table outright, because all five
//! calls expand to nothing, and takes the linked ELF from 122 defmt symbols down to 17. Flipping
//! the variable triggers that rebuild on its own with no `cargo clean`: defmt-macros emits
//! `rerun-if-env-changed=DEFMT_LOG` and leaves an `option_env!("DEFMT_LOG")` at every call site.
//!
//! What it cannot do is separate facade records from one another. Every `log::` call in the
//! firmware arrives at the five macro sites in *this* module, so a module-path filter sees
//! `asperitas_logging::defmt_log` and nothing else. The facade therefore has one knob, and it
//! works on level: `DEFMT_LOG=warn` keeps WARN and ERROR from everywhere. Drivers that speak
//! defmt directly, such as the `stm32-metapac` derives behind its `defmt` feature, stay
//! individually addressable by path.
//!
//! Reaching for that path syntax has one sharp edge: a value the parser dislikes does not fail
//! where you can see it. `DEFMT_LOG=off,crate=off` produced roughly 300 `proc macro panicked:
//! `crate` is not a valid identifier` errors inside `stm32-metapac`'s generated register code,
//! none of them naming `DEFMT_LOG` or this crate. A bare level is the safe form.

use log::Level;

use crate::console;

/// Largest defmt frame this bridge can produce, in encoded bytes.
///
/// The body window is the whole budget: defmt-rtt's ring holds `BUF_SIZE - 1` usable bytes and
/// `BUF_SIZE` defaults to **1024** (defmt-rtt 1.3.0 build.rs, overridable with
/// `DEFMT_RTT_BUFFER_SIZE`). A frame is the body plus defmt's own overhead — a header byte
/// carrying level and tag, a varint interned-format index, and the argument bytes the encoder
/// inserts for `{}` — which is single-digit for a one-argument frame. So the worst case here is
/// roughly 256 + 8 = 264 bytes against 1023 usable: the window cannot overflow the ring, and
/// `BUF_SIZE` is deliberately left at its default.
///
/// That matters more than it looks. In the default (non-`drop-on-contention`) build an
/// over-sized frame is not dropped: `Channel::write_all` loops until every byte lands, and
/// `write_impl` refuses to write a chunk of `BUF_SIZE` or more (channel.rs:93), so such a frame
/// spins forever inside a critical section. The margin above is what keeps that path unreachable
/// rather than merely unlikely.
const MAX_FRAME_BODY: usize = console::BODY_WINDOW;

/// Deliver one `log::Record` as one defmt frame.
///
/// The buffer is a local, not a shared static: rendering then emitting cannot be atomic across
/// contexts without a lock, and a diagnostic path is not worth a second mutex next to the one
/// the USB path already carries. 256 bytes of transient stack follows the precedent set by
/// [`crate::panic_handler`], which puts its own message buffer on the panic stack for the same
/// reason.
pub(crate) fn emit(record: &log::Record) {
    let mut buf = [0u8; MAX_FRAME_BODY];
    let body_len = crate::format_body(record, &mut buf);
    emit_frame(record.level(), &buf[..body_len]);
}

/// Deliver an already-formatted panic message.
///
/// The panic handler has a `PanicInfo`, not a `log::Record`, so it cannot come through
/// [`emit`]. Same frame, same level mapping, one fewer conversion.
///
/// Reaching defmt from a panic that happened *inside* a defmt frame hits defmt-rtt's
/// reentrancy check and panics again — a recursive panic, not a quiet loss. Nothing here can
/// detect that without keeping state the handler would have to trust after a stack unwind, so
/// the trade stands as it does for the LED write beside it: best effort, and TASK-037 is where
/// anyone finds out whether it bites.
pub(crate) fn emit_panic(body: &[u8]) {
    emit_frame(Level::Error, body);
}

/// Render bytes as UTF-8 and emit them at `level` in a single defmt frame.
///
/// Invalid UTF-8 becomes a fixed marker rather than a partial frame: a body that cannot be shown
/// is worth less than a body that announces it was unreadable.
fn emit_frame(level: Level, body: &[u8]) {
    let Ok(text) = core::str::from_utf8(body) else {
        match level {
            Level::Error => defmt::error!("{}", "<invalid utf8>"),
            Level::Warn => defmt::warn!("{}", "<invalid utf8>"),
            Level::Info => defmt::info!("{}", "<invalid utf8>"),
            Level::Debug => defmt::debug!("{}", "<invalid utf8>"),
            Level::Trace => defmt::trace!("{}", "<invalid utf8>"),
        }
        return;
    };

    // defmt filters at compile time from `DEFMT_LOG`, unlike the facade's runtime
    // `set_max_level`, and a filtered call expands to nothing — so a level the host did not ask
    // for costs no bytes and no cycles, not merely no output.
    match level {
        Level::Error => defmt::error!("{}", text),
        Level::Warn => defmt::warn!("{}", text),
        Level::Info => defmt::info!("{}", text),
        Level::Debug => defmt::debug!("{}", text),
        Level::Trace => defmt::trace!("{}", text),
    }
}
