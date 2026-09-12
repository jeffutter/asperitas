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
//!   to the most verbose defmt level, discarding the information this crate exists to carry. Its
//!   Maturity section names a second problem in the same breath: it "uses a fixed size buffer",
//!   and "will likely introduce such features (altering its behavior) without declaring breaking
//!   changes". The buffer is not the objection on its own — [`emit_frame`] bounds its body to
//!   `console::BODY_WINDOW` too, deliberately, so that the worst-case frame fits the ring and a
//!   compile-time assert below checks that it does.
//!   The objection is that this one's bound is an undocumented shortcut its maintainers reserve the
//!   right to change without calling it a breaking change.
//! - `defmt2log` 0.2.1 — not a candidate at all, because it runs the other way: a `defmt::Logger`
//!   that decodes defmt frames into `log` records on the host (`defmt-decoder`, reading the ELF's
//!   `.defmt` section). Right for keeping defmt call-sites in code that also builds for the host;
//!   no use reaching from `log` to a probe.
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
//! (defmt-macros-1.1.1 src/function_like/log/env_filter.rs:35), so four of this module's five arms
//! expand to nothing and only `log::error!` survives. Measured 2026-09-12 on `main.bin` from
//! `make build NO_DEFAULT=1 FEATURES="seed3 log-defmt"`, at the shipped `[profile.release]
//! debug = 2`: an unset build and a `DEFMT_LOG=error` build are the same
//! size exactly (48320 bytes); `warn` is 48376, `debug` 48380, `trace` 49748, `off` 44740, and
//! `info` 47712 - below the error-only baseline, not above it.
//!
//! Treat those as one set of numbers at one DWARF level. The same seven builds at
//! `debug = "line-tables-only"` came out 48084 (unset and `error` alike), `warn` 48136, `info`
//! 48304, `debug` 48984, `trace` 49712, `off` 44592, with `info` above the baseline instead of
//! under it. Debug info perturbs codegen - TASK-054's table in `firmware/Cargo.toml` is that
//! measurement - so a size claim here is good for the profile it was taken at, and re-measuring
//! costs about 20 seconds.
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

/// defmt-rtt 1.3.0's ring size: its `build.rs` defaults `BUF_SIZE` to 1024 and honours
/// `DEFMT_RTT_BUFFER_SIZE`, which nothing in this workspace reads.
///
/// Declared locally because `asperitas-logging` depends on `defmt` but not on `defmt-rtt` - the
/// logger sits in the binary (`firmware/Cargo.toml`), so this crate cannot name the real
/// constant. If anyone ever sets `DEFMT_RTT_BUFFER_SIZE` smaller, this number has to move with it
/// and the assert below is what notices.
const RTT_BUF_SIZE: usize = 1024;

/// Bytes the ring can actually hold: one less than its size, which is how defmt-rtt distinguishes
/// full from empty (`available_buffer_size`, `channel.rs:158-164`).
const RTT_RING_USABLE: usize = RTT_BUF_SIZE - 1;

/// defmt's own framing around a body, in bytes before rzcobs: the header byte carrying level and
/// tag, a varint interned-format index, and the length a `{}` str argument costs - a fixed 4-byte
/// little-endian `u32`, not a varint (`defmt` 1.1.1 `export/mod.rs`'s `str()` calls `usize()`, and
/// `integers.rs`'s `usize()` writes `(*b as u32).to_le_bytes()`).
const FRAME_OVERHEAD_BYTES: usize = 8;

/// Worst encoded size of one frame from this bridge: the window, plus defmt's overhead, expanded
/// by rzcobs, which spends roughly one output byte per seven payload bytes and adds a frame
/// separator (`defmt` 1.1.1 `encoding/rzcobs.rs:27-53`; the raw encoding would be smaller, but
/// nothing here enables `encoding-raw`).
const WORST_ENCODED_FRAME: usize = (MAX_FRAME_BODY + FRAME_OVERHEAD_BYTES) * 8 / 7 + 2;

// One frame must fit the ring whole. Frame boundaries are what a reader recovers after dropping
// bytes, so a frame that cannot fit is not merely late - non-blocking mode truncates it
// (`channel.rs:64-69`) and blocking mode spends the whole interrupt-off window failing to deliver
// it. Spelled into the string rather than formatted: a `format_args` message is not a const
// expression (E0015).
const _: () = assert!(
    WORST_ENCODED_FRAME < RTT_RING_USABLE,
    "worst-case defmt frame exceeds defmt-rtt's usable ring"
);

/// Largest defmt frame this bridge can produce, in bytes of rendered body.
///
/// The body window is the whole budget, and the arithmetic above pins it: worst case is 264 bytes
/// pre-expansion, 304 encoded, against 1023 usable. `BUF_SIZE` is deliberately left at its
/// default.
///
/// What the margin buys is occupancy and interrupt-off time, not reachability. The ring never
/// receives a whole frame at once: defmt-rtt's logger forwards each encoder callback straight to
/// `Channel::write_all` (`defmt-rtt` `src/lib.rs:166-186`), and the default rzcobs encoder drives
/// that closure one byte at a time (`defmt` 1.1.1 `encoding/rzcobs.rs:57`). So the size of a frame
/// cannot decide whether a write fits - ring occupancy against the host's drain rate decides that,
/// and when occupancy wins the target spins: `write_all` loops while `!bytes.is_empty()`
/// (`channel.rs:38-42`) calling `blocking_write`, which returns 0 at `available == 0`
/// (`channel.rs:57-59`), all of it between `critical_section::acquire()` and its `release()`
/// (`src/lib.rs:168`, `:239`). A shorter frame bounds two of those consequences: how much of the
/// 1023 bytes one log line can take, hence how long a stalled host takes to fill the ring, and how
/// long interrupts stay off while it fills it.
///
/// It does not make the spin unreachable, which an earlier version of this comment claimed on the
/// strength of a `write_impl` that refuses chunks of `BUF_SIZE` or more. That refusal is
/// `channel.rs:93`, inside the `#[cfg(feature = "drop-on-contention")] impl Channel` block that
/// begins at `channel.rs:90` and that this build does not compile. The default `write_impl`
/// (`channel.rs:71-87`) clamps at `bytes.len().min(available)` and refuses nothing; for what the
/// cfg'd-out mode means, see its documentation at `defmt-rtt/src/lib.rs:42-52`.
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
