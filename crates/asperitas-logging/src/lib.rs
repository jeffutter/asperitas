//! Logging facade for Asperitas firmware.
//!
//! Provides a feature-selected logging backend using the `log` crate.
//! Call-sites use `info!`, `debug!`, `warn!`, `error!` from the re-exported macros.
//! Backend selection is entirely at init time via Cargo features.
//!
//! # Features
//!
//! - `log-usb` — USB CDC-ACM serial logging over the Seed3's onboard USB-C
//! - `log-defmt` — defmt frames over RTT, read through a debug probe
//!
//! When no backend feature is enabled, logging falls back to a no-op logger.
//!
//! # Both transports at once
//!
//! `log-usb` and `log-defmt` may be on together, and then the facade keeps pointing at the
//! framed console: the capture tooling parses that stream's sequence numbers and loss counters,
//! which defmt cannot carry. RTT is not idle in that configuration — daisy-embassy and
//! embassy-stm32 call `defmt::info!` directly, so driver chatter reaches the probe as soon as
//! `defmt-rtt` is linked in the binary, whatever [`Backend`] says. Records from `log::info!`
//! therefore land on USB while `defmt::info!` from a driver lands on RTT. That split is the
//! design, not a misconfiguration.
//!
//! # Diagnostics features
//!
//! The non-logging diagnostics are selected by their own feature rather than by whichever
//! transport happens to be on:
//!
//! - `boot-led` — the `led` boot-stage indicator and the shared `panic_handler`
//!
//! `log-usb` enables `boot-led`, so a configuration with the serial transport keeps the LED
//! and the panic handler. Enabling `boot-led` alone serves the board that dies before any host
//! attaches: there is nobody to receive bytes, but a colour on a pin still says how far boot
//! got.
//!
//! # Record path (`log-usb`)
//!
//! ```text
//! log::info!("msg") → FacadeLogger → [one critical section: format → frame → space check
//!                                     → commit] → Pipe → usb::run() drain task → CDC-ACM
//! ```
//!
//! Everything inside the brackets happens with interrupts disabled and ends with either
//! the whole record in the ring or none of it — the one way that rule can break is a sink
//! that stalls mid-frame, which exits through the panic handler instead of the normal
//! return path. That is what lets a capture distinguish "this record was never generated"
//! from "this record was lost". See [`emit`] and [`frame::write_whole`].

#![no_std]

pub use log::{debug, error, info, trace, warn, Level, LevelFilter};

use core::sync::atomic::{AtomicBool, Ordering};

/// Has the logger been installed?
static LOGGER_INSTALLED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Global logger — implements log::Log
// ---------------------------------------------------------------------------

/// Backend enum — selected at compile time by features.
pub(crate) enum Backend {
    NoOp,
    #[cfg(feature = "log-usb")]
    Usb,
    #[cfg(feature = "log-defmt")]
    Defmt,
}

impl Backend {
    /// Deliver one record.
    ///
    /// The `NoOp` arm is the state before anything has switched the backend away from it —
    /// for the USB transport, that is [`usb::init`]. Those records vanish **uncounted**, because there is no console yet to attribute them
    /// to. Counting them would make `sent + dropped_full` disagree with what the host can
    /// ever see, for a reason nobody watching a capture can act on. Leave it that way;
    /// do not file it as a counter bug.
    #[allow(unused_variables)]
    pub(crate) fn write(&self, record: &log::Record) {
        match self {
            Backend::NoOp => {}
            #[cfg(feature = "log-usb")]
            Backend::Usb => emit_log_record(record),
            #[cfg(feature = "log-defmt")]
            Backend::Defmt => defmt_log::emit(record),
        }
    }
}

/// The global logger instance.
static mut GLOBAL_BACKEND: Backend = Backend::NoOp;

/// Read the active backend.
///
/// `addr_of_mut!` instead of `&GLOBAL_BACKEND`: taking a reference to a `static mut` is
/// precisely what the `static_mut_refs` lint forbids, and every static in this crate can
/// be reached without it — so the crate keeps no blanket `allow` that would also hide the
/// next real misuse.
fn backend() -> &'static Backend {
    // Safety: written exactly once, by `set_backend_usb`, while the boot thread is still
    // alone (before the executor runs and before any other task exists), and read-only
    // thereafter. Single-core, so no observer can straddle the write.
    unsafe { &*core::ptr::addr_of_mut!(GLOBAL_BACKEND) }
}

/// Switch logging to the USB backend.
///
/// Called by [`usb::init`] after the device and endpoint storage are live. Anything
/// logged before this call reaches [`Backend::NoOp`] and is discarded.
#[cfg(feature = "log-usb")]
pub(crate) fn set_backend_usb() {
    // Safety: see [`backend`] — single-threaded boot, one-way transition.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(GLOBAL_BACKEND), Backend::Usb);
    }
}

/// Switch logging to the defmt backend.
///
/// Unlike [`set_backend_usb`] there is no device to wait for: RTT is a couple of statics in
/// RAM that exist from the first instruction, which is why this is the only channel that can
/// speak during the pre-USB window. Callers reach it through [`init`]; it stays public because
/// a binary that wants RTT before anything else can call it directly.
#[cfg(feature = "log-defmt")]
pub fn set_backend_defmt() {
    // Safety: see [`backend`] — single-threaded boot, one-way transition.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(GLOBAL_BACKEND), Backend::Defmt);
    }
}

struct FacadeLogger;

impl log::Log for FacadeLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        backend().write(record);
    }

    fn flush(&self) {}
}

/// A `fmt::Write` that stops at the end of its buffer instead of overflowing it.
///
/// Log bodies may neither allocate nor panic, so running past the window has to be a
/// quiet stop rather than an error path. A piece that does not fit at all returns
/// [`core::fmt::Error`], which aborts the enclosing `write!` and leaves whatever already
/// landed — a shortened record, which the encoder then reports on the wire as `trunc`.
pub(crate) struct TruncWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> TruncWriter<'a> {
    pub(crate) fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes written so far.
    pub(crate) fn filled(&self) -> usize {
        self.pos
    }
}

impl core::fmt::Write for TruncWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        let available = self.buf.len() - self.pos;
        let len = s.len().min(available);
        if len == 0 {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + len].copy_from_slice(&s.as_bytes()[..len]);
        self.pos += len;
        Ok(())
    }
}

/// Install the facade logger as the global `log` backend.
pub(crate) fn install_logger() {
    if LOGGER_INSTALLED.swap(true, Ordering::AcqRel) {
        return; // Already installed
    }

    log::set_logger(&FacadeLogger)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .ok();
}

// ---------------------------------------------------------------------------
// Ungated modules — pure logic, no hardware types, host-testable by default
// ---------------------------------------------------------------------------

/// Console protocol v1: self-verifying framed log records.
///
/// Deliberately **not** behind `log-usb`: the codec is pure byte arithmetic that
/// CI must exercise on the host (`cargo test --workspace` builds this crate with
/// default features, i.e. without any backend).
pub mod frame;

/// Sequence numbers, loss counters, and the `BOOT`/`STATUS` wire text.
///
/// Ungated for the same reason as [`frame`]: the field set is a contract the host parses,
/// so its tests must run without hardware features enabled.
pub mod console;

/// Base64 payload codec for shipping captured audio over the framed console.
///
/// Ungated for the same reason as [`frame`]: it is pure byte arithmetic whose equivalence
/// with a reference implementation is proven on the host, where the oracle can be a
/// dev-dependency.
pub mod dump;

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

// Gated on `any(feature = "log-usb", test)` rather than `log-usb` alone: its arithmetic
// must be testable under default features, which is the only configuration CI's
// `cargo test --workspace` builds. See the module docs.
#[cfg(any(feature = "log-usb", test))]
mod spin_budget;

#[cfg(feature = "log-usb")]
pub mod usb;

/// defmt transport: one `log::Record` in, one defmt frame out. See the module docs.
#[cfg(feature = "log-defmt")]
mod defmt_log;

/// Boot-stage LED indicator and its blink task. See the module docs.
#[cfg(feature = "boot-led")]
pub mod led;

/// Shared `#[panic_handler]` body. See the module docs.
#[cfg(feature = "boot-led")]
pub mod panic_handler;

// ---------------------------------------------------------------------------
// USB commit path — a record is indivisible here, and nowhere else
// ---------------------------------------------------------------------------

/// Pipe buffer size — holds several log messages before backpressure.
///
/// Sized for the burst that arrives between two drains, not for one record: at 2048 B the
/// ring holds nine maximum-size frames, against a 512 B ring that held two and dropped
/// records during ordinary task-switch latency. Beyond that, more buffering mostly means a
/// stalled host replays stale bytes for longer.
///
/// Deliberately **not** behind `log-usb`: it is a bare integer with no dependency on
/// `embassy_sync`, and the host suite that proves the dump headroom rule
/// (`tests/console_dump.rs`) has to sweep the same ring the device builds. Gating it would
/// force that test to restate 2048 as its own constant, where the two could disagree in
/// silence. The static that does carry a type-level dependency, `LOG_PIPE`, stays gated.
pub const LOG_PIPE_SIZE: usize = 2048;

/// The log pipe: [`emit`] commits framed records, [`usb`]'s drain task empties it.
///
/// A plain immutable `static`, because `Pipe`'s methods all take `&self` and its own
/// `CriticalSectionRawMutex` serialises them. The previous `Option` reached through
/// `&raw mut` existed only to defer construction that `Pipe::new()` performs in `const`
/// context anyway, and cost an aliasing hack plus a crate-wide lint allow.
///
/// `CriticalSectionRawMutex` rather than `NoopRawMutex`: writes now happen inside our own
/// IRQ-off region, where a no-op mutex would let an interrupt producer interleave bytes
/// into the middle of a record. Nesting is documented-safe — `critical_section::with`
/// saves and restores `PRIMASK`, and an inner section inside an outer one is a no-op.
///
/// Every mention of this type stays behind `log-usb`: on the host no `critical-section`
/// implementation is registered, which fails at *link* time with an undefined symbol
/// rather than a type error.
#[cfg(feature = "log-usb")]
pub static LOG_PIPE: embassy_sync::pipe::Pipe<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    LOG_PIPE_SIZE,
> = embassy_sync::pipe::Pipe::new();

/// The two buffers a record needs, guarded by one lock.
///
/// Two buffers, not one: [`frame::encode`] takes `body` and `out` as disjoint borrows, so
/// building a record in place is not expressible against the codec as built. The price is
/// one ≤256-byte copy per record — under a microsecond at 480 MHz, against a ~667 µs
/// audio block period.
#[cfg(feature = "log-usb")]
struct RecordBufs {
    body: [u8; console::BODY_WINDOW],
    frame: [u8; frame::MAX_FRAME],
}

#[cfg(feature = "log-usb")]
static RECORD_BUFS: embassy_sync::blocking_mutex::Mutex<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    core::cell::UnsafeCell<RecordBufs>,
> = embassy_sync::blocking_mutex::Mutex::new(core::cell::UnsafeCell::new(RecordBufs {
    body: [0; console::BODY_WINDOW],
    frame: [0; frame::MAX_FRAME],
}));

/// Run `commit` with the record buffers held, and fail loud if the pipe stalled mid-frame.
///
/// The stall panic lives here, after the record lock has released, and nowhere else. That
/// lock is a `CriticalSectionRawMutex`, so its closure runs with `PRIMASK` set, and this
/// target aborts rather than unwinds: a panic raised inside that closure never restores
/// `PRIMASK`, so the panic handler's serial emit (`usb::emit_blocking`) would spin with no USB
/// interrupt and drop the very text it exists to deliver. Callers therefore report
/// [`frame::WriteOutcome::Stalled`] out of their closure and let this function crash.
/// Returns whether the frame reached the pipe.
#[cfg(feature = "log-usb")]
fn commit_records(commit: impl FnOnce(&mut RecordBufs) -> frame::WriteOutcome) -> bool {
    let outcome = RECORD_BUFS.lock(|cell| {
        // Safety: the only route to these buffers is this mutex, the core is single-core,
        // and the reference never escapes this closure.
        commit(unsafe { &mut *cell.get() })
    });
    if outcome == frame::WriteOutcome::Stalled {
        panic!("record commit: the sink stalled after the capacity pre-check passed; the caller held RECORD_BUFS and verified free_capacity, so the pipe broke write_whole's contract and a record was lost");
    }
    outcome == frame::WriteOutcome::Committed
}

/// Format, frame, and commit one record — whole, or not at all.
///
/// `fill` writes the body into the guarded window and returns its length; the level,
/// sequence number, timestamp, checksum, and delivery are this function's business.
///
/// # What the critical section costs, bounded rather than measured
///
/// All of this runs with interrupts disabled while a 48 kHz audio block arrives every
/// ~667 µs on the same single-threaded executor. The work is fixed-size:
///
/// | step | bound |
/// |---|---|
/// | format into the body window | ≤ 256 bytes copied |
/// | sanitise + copy into the frame | ≤ 200 bytes in, ≤ 228 bytes out |
/// | CRC-16 over the CRC-covered range | ≤ 220 bytes × 8 bit iterations ≈ 1 760 shift-and-xor steps |
/// | commit | ≤ 2 `try_write` calls (one, plus one to cross a ring wrap) |
///
/// The table-less CRC dominates the region by a wide margin. This crate cannot measure
/// any of it — the time driver ticks at 32 768 Hz, so one tick (~30.5 µs) is coarser than
/// the window worth measuring and a `max_ticks` field would sit at zero — which is why the
/// bound above is stated in bytes and iterations instead of microseconds. If this region
/// ever shows up in the audio budget, the fix is a reserve/commit ring replacing the
/// `Pipe`, tracked as its own ticket, not a quiet redesign of the atomicity rule.
#[cfg(feature = "log-usb")]
fn emit(level: Level, fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize) {
    // The clock is read BEFORE the lock: it keeps the timer driver's own locking out of
    // the IRQ-off window, and a millisecond of skew is invisible in a millisecond field.
    // Everything that must agree with wire order — seq, frame, space check, commit — is
    // inside.
    let now_ms = embassy_time::Instant::now().as_millis() as u32;

    // Result ignored: this path counts the verdict internally, via the counters below.
    commit_records(|bufs| {
        let seq = console::CONSOLE.take_seq();
        let body_len = fill(&mut bufs.body);
        let encoded = frame::encode(level, seq, now_ms, &bufs.body[..body_len], &mut bufs.frame);
        if encoded.truncated {
            console::CONSOLE.body_shortened();
        }

        let framed = &bufs.frame[..encoded.len];
        // The capacity pre-check has to live inside this lock, next to the write loop.
        // Outside it, a full ring turns a clean headroom refusal into a mid-frame stall.
        // Inside, free capacity is *frozen* — `RECORD_BUFS.lock` masks interrupts globally,
        // so the drain consumer cannot run at all while the lock is held — and a pre-check
        // that passes therefore guarantees progress on every round.
        let outcome = frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
            LOG_PIPE.try_write(chunk).ok()
        });
        match outcome {
            frame::WriteOutcome::Committed => console::CONSOLE.record_committed(),
            frame::WriteOutcome::RefusedForSpace => {
                console::CONSOLE.record_dropped_for_space(framed.len())
            }
            // Counted as nothing: `commit_records` crashes on this once the lock releases.
            frame::WriteOutcome::Stalled => {}
        }
        outcome
    });
}

/// Commit a `log::Record` as one framed console record.
#[cfg(feature = "log-usb")]
fn emit_log_record(record: &log::Record) {
    emit(record.level(), |body| format_body(record, body))
}

/// Format the message half of a record.
///
/// No `[LEVEL]` prefix and no trailing CRLF, unlike the old line-oriented formatter: the
/// level travels in the frame header and CRLF is the frame's delimiter, so repeating
/// either inside the body would double-book information the decoder already trusts.
///
/// Shared by both transports on purpose — one rendering rule, two ways to ship it. A record
/// that reads differently over a probe than it does over the console would make the two
/// channels impossible to compare during a bench session.
#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
fn format_body(record: &log::Record, out: &mut [u8; console::BODY_WINDOW]) -> usize {
    use core::fmt::Write;

    let mut w = TruncWriter::new(out);
    let _ = core::write!(w, "{}", record.args());
    w.filled()
}

/// Emit the `BOOT` banner: proto, firmware version, ring size, body cap.
///
/// Call this *after* [`set_backend_usb`], or [`Backend::NoOp`] swallows it silently — the
/// one way this record can be lost for reasons the counters will not show. It rides the
/// normal commit path, so it consumes `seq 0` and can be dropped like any other record.
#[cfg(feature = "log-usb")]
pub(crate) fn emit_boot() {
    emit(Level::Info, |body| {
        console::boot_body(
            body,
            env!("CARGO_PKG_VERSION"),
            LOG_PIPE_SIZE,
            frame::MAX_BODY,
        )
    });
}

/// Emit one `STATUS` record reporting `snap`, taken by the caller just before deciding.
///
/// Rides the normal commit path, which is how its own emission gets counted and why a
/// `STATUS` record lost to a full ring behaves exactly like any other loss.
#[cfg(feature = "log-usb")]
pub(crate) fn emit_status(snap: &console::ConsoleCounters) {
    emit(Level::Info, |body| {
        console::status_body(body, snap, LOG_PIPE.free_capacity())
    });
}

/// Commit one pre-built audio-dump body as a framed record, or refuse it whole.
///
/// The only route by which dump traffic reaches [`LOG_PIPE`], and the only place
/// [`dump::dump_fits`]'s headroom rule is acted on. Synchronous and non-blocking: it either
/// commits or returns `false` having written nothing, so a caller on a timer can retry it
/// without risking a deadlock in whatever context it runs (TASK-038.03 owns that retry
/// loop, and keeps the dump task off the audio `InterruptExecutor`).
///
/// # Why each step sits where it does
///
/// - **The clock is read before the lock**, as [`emit`] does it, keeping the timer driver's
///   own locking out of the IRQ-off window. A millisecond of skew is invisible in a
///   millisecond field; sequence order is not, so `seq` is taken inside.
/// - **The lock is not optional.** Log records are emitted from arbitrary context including
///   the audio callback, so an interrupt can preempt a producer that does not hold
///   [`RECORD_BUFS`], and two interleaved `write_whole` calls splice two frames into the
///   ring. While the lock is held the consumer does *not* run: `RECORD_BUFS` is a
///   `CriticalSectionRawMutex`, which masks interrupts globally for its duration, so
///   `usb::run()`'s drain task cannot move a byte and free capacity is *frozen* — not merely
///   non-decreasing — for the duration. That is what makes the capacity check below sound
///   rather than optimistic. And `write_whole` no longer panics anywhere: a stall surfaces
///   as a `frame::WriteOutcome::Stalled` value and `commit_records` crashes on it only once
///   the critical section has released.
/// - **`dump_fits` is consulted before `take_seq`.** Refusals therefore consume no sequence
///   number, by construction rather than by discipline: a retry loop cannot manufacture `seq`
///   gaps that a host would read as loss. The predicate is tested exhaustively across the
///   ring's whole capacity range on the host, so the decision itself is proven even though
///   nothing behind `log-usb` is reachable from CI (see the coverage note below).
/// - **Committed dumps do consume `seq` and do bump `records_sent`.** Those records really
///   occupy the wire, and `seq` continuity must keep meaning loss for the audio stream to be
///   trustworthy; TASK-038.06 documents the resulting mixed-record rate for bench operators.
/// - **Refusals bump neither `dropped_full` nor `bytes_dropped`.** Those counters mean "a
///   record was thrown away", and a retry is not that. If dump stalls ever need their own
///   counter they belong to TASK-038.03's starvation counters, not these.
/// - **`Pipe::write` / `Pipe::write_all` are not used.** Either strands a partial frame in
///   the ring whenever capacity falls short of the frame, and the pipe has one shared
///   `write_waker` woken only on the full→non-full transition, so a stranded prefix wakes
///   nobody. See TASK-038.02's notes on `ready_send`, which embassy-sync 0.6.2 does not have.
///
/// # What CI can and cannot reach
///
/// `cargo test --workspace` builds this crate without `log-usb`, and on the host no
/// `critical-section` implementation is registered, so this function does not merely fail to
/// run there — it fails to *link*. What CI proves is the predicate ([`dump::dump_fits`],
/// exhaustively, against a real `embassy_sync` ring: see `tests/console_dump.rs`) and the
/// shape this function mirrors from [`emit`]. What only the firmware release build
/// (`cd firmware && cargo build --release --features seed3`) proves is that the arrangement
/// type-checks at all. Neither reaches runtime behaviour on hardware; TASK-038.05 is the
/// human-run check of that.
///
/// A body longer than [`frame::MAX_BODY`] is refused outright rather than shipped shortened:
/// a chunk whose tail silently vanished reassembles into audio that measures wrong, and the
/// block CRC catching it afterwards is not the same thing as not sending it. Every builder in
/// [`dump`] writes into a `[u8; frame::MAX_BODY]` window, so reaching this branch is a caller
/// bug — hence the debug assertion alongside the refusal. The check runs *before* the record
/// lock is taken, so that assertion cannot fire with interrupts masked.
#[cfg(feature = "log-usb")]
pub fn try_emit_dump(body: &[u8]) -> bool {
    let now_ms = embassy_time::Instant::now().as_millis() as u32;

    // Pure input validation over an argument — no shared state, so it stays out of the
    // critical section, where even a debug-profile panic would leave PRIMASK stuck.
    if body.len() > frame::MAX_BODY {
        debug_assert!(
            false,
            "dump body of {} bytes exceeds MAX_BODY; refusing rather than shipping a shortened chunk",
            body.len(),
        );
        return false;
    }

    commit_records(|bufs| {
        // The headroom rule, asked before anything is spent: no `seq`, no counter, no byte.
        if !dump::dump_fits(body.len(), LOG_PIPE.free_capacity()) {
            return frame::WriteOutcome::RefusedForSpace;
        }

        let seq = console::CONSOLE.take_seq();
        let encoded = frame::encode(Level::Info, seq, now_ms, body, &mut bufs.frame);
        debug_assert!(
            !encoded.truncated,
            "a body checked against MAX_BODY cannot arrive truncated"
        );

        let framed = &bufs.frame[..encoded.len];
        let outcome = frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
            LOG_PIPE.try_write(chunk).ok()
        });
        match outcome {
            frame::WriteOutcome::Committed => console::CONSOLE.record_committed(),
            frame::WriteOutcome::RefusedForSpace => {
                // Unreachable while the lock holds the producer contract: free capacity is
                // frozen here, and the pre-check above already paid the reserve. Reaching it
                // means the sink broke that contract, and then a record genuinely was lost —
                // so this one path does count, unlike a headroom refusal.
                debug_assert!(
                    false,
                    "pipe refused a {}-byte frame the headroom rule had already admitted",
                    framed.len(),
                );
                console::CONSOLE.record_dropped_for_space(framed.len());
            }
            // Counted as nothing: `commit_records` crashes on this once the lock releases.
            frame::WriteOutcome::Stalled => {}
        }
        outcome
    })
}

// ---------------------------------------------------------------------------
// Default init (no features)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "log-usb"))]
/// Initialize the logging facade with whichever backend this build has.
///
/// The entry point for every configuration except the USB console, which initializes itself
/// ([`usb::init`] installs the logger as part of bringing up the device). Picks defmt when
/// `log-defmt` is on — RTT needs no device, so there is nothing to wait for — and a no-op
/// otherwise, in which case every message is silently dropped.
pub fn init() {
    #[cfg(feature = "log-defmt")]
    set_backend_defmt();
    install_logger();
}
