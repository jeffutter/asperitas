//! Logging facade for Asperitas firmware.
//!
//! Provides a feature-selected logging backend using the `log` crate.
//! Call-sites use `info!`, `debug!`, `warn!`, `error!` from the re-exported macros.
//! Backend selection is entirely at init time via Cargo features.
//!
//! # Features
//!
//! - `log-usb` — USB CDC-ACM serial logging over the Seed3's onboard USB-C
//! - (future) `log-defmt` — defmt-based logging for probe-based debugging
//!
//! When no backend feature is enabled, logging falls back to a no-op logger.
//!
//! # Record path (`log-usb`)
//!
//! ```text
//! log::info!("msg") → FacadeLogger → [one critical section: format → frame → space check
//!                                     → commit] → Pipe → usb::run() drain task → CDC-ACM
//! ```
//!
//! Everything inside the brackets happens with interrupts disabled and ends with either
//! the whole record in the ring or none of it, which is what lets a capture distinguish
//! "this record was never generated" from "this record was lost". See [`emit`] and
//! [`frame::write_whole`].

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
}

impl Backend {
    /// Deliver one record.
    ///
    /// The `NoOp` arm is the state before [`usb::init`] switches the backend: those
    /// records vanish **uncounted**, because there is no console yet to attribute them
    /// to. Counting them would make `sent + dropped_full` disagree with what the host can
    /// ever see, for a reason nobody watching a capture can act on. Leave it that way;
    /// do not file it as a counter bug.
    #[allow(unused_variables)]
    pub(crate) fn write(&self, record: &log::Record) {
        match self {
            Backend::NoOp => {}
            #[cfg(feature = "log-usb")]
            Backend::Usb => emit_log_record(record),
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

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "log-usb")]
pub mod usb;

#[cfg(feature = "log-usb")]
pub mod led;

#[cfg(feature = "log-usb")]
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
#[cfg(feature = "log-usb")]
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

    RECORD_BUFS.lock(|cell| {
        // Safety: the only route to these buffers is this mutex, the core is single-core,
        // and the reference never escapes this closure.
        let bufs = unsafe { &mut *cell.get() };

        let seq = console::CONSOLE.take_seq();
        let body_len = fill(&mut bufs.body);
        let encoded = frame::encode(level, seq, now_ms, &bufs.body[..body_len], &mut bufs.frame);
        if encoded.truncated {
            console::CONSOLE.body_shortened();
        }

        let framed = &bufs.frame[..encoded.len];
        // The capacity pre-check has to live inside this lock, next to the write loop.
        // Outside it, a full ring turns `write_whole`'s stall assertion into a debug-build
        // panic instead of a clean drop; inside, the consumer can only ever *increase* free
        // capacity, so a pre-check that passes guarantees progress on every round.
        if !frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
            LOG_PIPE.try_write(chunk).ok()
        }) {
            console::CONSOLE.record_dropped_for_space(framed.len());
            return;
        }

        console::CONSOLE.record_committed();
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
#[cfg(feature = "log-usb")]
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

// ---------------------------------------------------------------------------
// Default init (no features)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "log-usb"))]
/// Initialize the logging facade with a no-op backend.
///
/// When no backend feature is enabled, all log messages are silently dropped.
pub fn init() {
    install_logger();
}
