//! Device logging runtime for Asperitas firmware: the log backends, the USB CDC console, the
//! panic handler and the status LEDs. The wire protocol it speaks lives in `asperitas-wire`.
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
//! `defmt-rtt` is linked in the binary, whatever the selected `Backend` says. Records from `log::info!`
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
//!                                     → commit] → Pipe → Console::run() drain task → CDC-ACM
//! ```
//!
//! Everything inside the brackets happens with interrupts disabled and ends with either
//! the whole record in the ring or none of it — the one way that rule can break is a sink
//! that stalls mid-frame, which exits through the panic handler instead of the normal
//! return path. That is what lets a capture distinguish "this record was never generated"
//! from "this record was lost". See the crate-private `emit_at` and [`frame::write_whole`].
//!
//! Three producers use that one commit path, differing only in who renders the body and who
//! chooses the timestamp: `log::Record`s go through `emit`, which reads the clock; whole
//! wire-contract bodies the caller already rendered go through `emit_record`, which takes the
//! timestamp as an argument; and bulk audio-dump chunks go through `try_emit_dump`, which also
//! takes it and additionally applies [`asperitas_wire::dump::dump_fits`]'s headroom rule before spending a
//! sequence number. The last two are named rather than linked, because both are
//! `log-usb`-gated and neither exists to resolve in the default-feature docs.

#![no_std]

pub use log::{debug, error, info, trace, warn, Level, LevelFilter};

// Only the `log-usb` record path calls into these; the doc links keep naming them in every
// configuration.
#[allow(unused_imports)]
use asperitas_wire::{console, frame};
use core::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Global logger - implements log::Log
// ---------------------------------------------------------------------------

/// Backend enum - selected at compile time by features.
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum Backend {
    NoOp = 0,
    #[cfg(feature = "log-usb")]
    Usb = 1,
    #[cfg(feature = "log-defmt")]
    Defmt = 2,
}

impl Backend {
    /// Deliver one record.
    ///
    /// The `NoOp` arm is the state before anything has switched the backend away from it -
    /// for the USB transport, that is [`init`]. Those records vanish **uncounted**, because there is no console yet to attribute them
    /// to. Counting them would make `sent + dropped_full` disagree with what the host can
    /// ever see, for a reason nobody watching a capture can act on. Leave it that way;
    /// do not file it as a counter bug.
    #[allow(unused_variables)]
    pub(crate) fn write(self, record: &log::Record) {
        match self {
            Backend::NoOp => {}
            #[cfg(feature = "log-usb")]
            Backend::Usb => emit_log_record(record),
            #[cfg(feature = "log-defmt")]
            Backend::Defmt => defmt_log::emit(record),
        }
    }
}

/// The active backend, as a `Backend` discriminant.
///
/// Written once by [`init`] and read by every record; an atomic rather than a `static mut`
/// because a record can be logged from interrupt context while `init` is still finishing.
static ACTIVE_BACKEND: AtomicU8 = AtomicU8::new(Backend::NoOp as u8);

fn backend() -> Backend {
    match ACTIVE_BACKEND.load(Ordering::Acquire) {
        #[cfg(feature = "log-usb")]
        x if x == Backend::Usb as u8 => Backend::Usb,
        #[cfg(feature = "log-defmt")]
        x if x == Backend::Defmt as u8 => Backend::Defmt,
        _ => Backend::NoOp,
    }
}

/// Point logging at `backend`. Anything logged before this call reaches [`Backend::NoOp`] and
/// is discarded.
#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
fn set_backend(backend: Backend) {
    ACTIVE_BACKEND.store(backend as u8, Ordering::Release);
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

/// Install the facade logger as the global `log` backend. `log::set_logger` refuses a second
/// install, which is the whole idempotence story.
fn install_logger() {
    log::set_logger(&FacadeLogger)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .ok();
}

// ---------------------------------------------------------------------------
// Ungated modules - pure logic, no hardware types, host-testable by default
// ---------------------------------------------------------------------------

pub mod inbound;

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

#[cfg(feature = "log-defmt")]
mod defmt_log;

#[cfg(feature = "boot-led")]
pub mod led;

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
/// (`tests/dump_headroom.rs`) has to sweep the same ring the device builds. Gating it would
/// force that test to restate 2048 as its own constant, where the two could disagree in
/// silence. The static that does carry a type-level dependency, `LOG_PIPE`, stays gated.
pub const LOG_PIPE_SIZE: usize = 2048;

/// The log pipe: `commit` puts framed records here, [`usb`]'s drain task empties it.
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
pub(crate) static LOG_PIPE: embassy_sync::pipe::Pipe<
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
/// Every panic on the record-commit path fires here, after the record lock has released, and
/// nowhere else. That lock is a `CriticalSectionRawMutex`, so its closure runs with `PRIMASK` set,
/// and this target aborts rather than unwinds: a panic raised inside that closure never restores
/// `PRIMASK`, so the panic handler's serial emit (`usb::emit_blocking`) would spin with no USB
/// interrupt and drop the very text it exists to deliver. The closure therefore reports a broken
/// contract as [`frame::WriteOutcome::Stalled`] and this function does the crashing. The functions
/// the closure runs (`frame::encode`, `frame::write_whole`, `asperitas_wire::dump::dump_fits`,
/// [`write_and_count`] and the console counters) carry
/// `#[deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]`, so
/// clippy rejects any of them that can panic through indexing, unwrap or an explicit panic. Release
/// builds wrap on overflow rather than panic, so arithmetic is not linted. Run
/// `cargo clippy -p asperitas-logging --features log-usb --target thumbv7em-none-eabihf` and
/// `cargo clippy -p asperitas-wire` to check; the firmware workspace does not lint these path crates.
///
/// A stall panics in every build: the alternative is an unbounded retry spin that produces no
/// diagnostic at all. A refusal for space is ordinary backpressure, counted into
/// `dropped_full`/`bytes_dropped` where `console::snapshot()` and STATUS report it.
///
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

/// Write one encoded frame into [`LOG_PIPE`] whole or not at all, and count what happened.
///
/// Must run inside the record lock: free capacity is only frozen there, because the lock masks
/// interrupts and the drain consumer cannot run. A pre-check outside the lock would turn a clean
/// refusal into a mid-frame stall. A refusal counts the loss into `dropped_full`/`bytes_dropped`;
/// a stall counts nothing, since [`commit_records`] crashes on it once the lock releases.
#[cfg(feature = "log-usb")]
#[deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
fn write_and_count(framed: &[u8]) -> frame::WriteOutcome {
    let outcome = frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
        LOG_PIPE.try_write(chunk).ok()
    });
    match outcome {
        frame::WriteOutcome::Committed => console::CONSOLE.record_committed(),
        frame::WriteOutcome::RefusedForSpace => {
            console::CONSOLE.record_dropped_for_space(framed.len())
        }
        frame::WriteOutcome::Stalled => {}
    }
    outcome
}

/// Commit one already-rendered record body as a whole frame, returning whether it reached the pipe.
///
/// The non-panic way to put bytes on the console that [`log`] cannot express. A caller holding a
/// wire-contract body (`RIGCFG`, `RIGGEN`, `CAPSTAT`, `CAPMAX`, `STIMSTART`, `DUMPEND` from
/// [`crate::console`]) needs those bytes to arrive unchanged, and `log::info!("{}", body)` would
/// hand them to a formatting path this crate does not control.
///
/// Takes `now_ms` rather than reading the clock, unlike every other producer here. A rig emits its
/// verbs around work whose duration *is* the measurement (`DUMPEND`'s `elapsed_ms`, `stall_ms`), so
/// stamping such a record after the fact with a clock read taken wherever the caller happened to
/// reach would put a timestamp on the wire that disagrees with the numbers beside it. Ordinary logs
/// have no such constraint and use `emit`, which reads the clock itself.
///
/// # Sibling, not substitute, for [`try_emit_dump`]
///
/// Both go through `commit_records` and differ in one decision, which is why they are two
/// functions rather than one with a flag. This is an ordinary record: if the pipe is full it is
/// dropped and counted in `ConsoleStats::dropped_full`, exactly as `BOOT` and `STATUS` are, because
/// a measurement record must never displace the traffic it describes. A dump chunk is bulk payload
/// that must fit *around* the reserve, and its writer retries a refusal instead of accepting the
/// loss. Same reason [`Level`] is a parameter here and fixed to `Info` there.
///
/// Callers pass `Level::Info`, matching `BOOT`/`STATUS`: these are device facts, not diagnostics,
/// and a Debug filter must not silence a measurement.
///
/// # Failure mode
///
/// Never panics. A body longer than [`console::BODY_WINDOW`] is clipped to it, which the encoder
/// counts as a truncation - the same convention [`console::status_body`] follows by truncating
/// rather than complaining. None of the five rig builders can reach that case, since each is
/// compile-time-bounded below [`frame::MAX_BODY`]; the clipping exists so a future caller's mistake
/// costs a shortened field rather than a stuck interrupt mask.
#[cfg(feature = "log-usb")]
pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool {
    emit_at(level, now_ms, |out| {
        let n = body.len().min(out.len());
        out[..n].copy_from_slice(&body[..n]);
        n
    })
}

/// Format, frame, and commit one record at the clock's current time.
///
/// Thin wrapper over [`emit_at`] that owns the one thing a caller must not decide for itself: what
/// time it is. Kept separate rather than defaulted at each call site because reading the clock
/// inside the record lock is the bug - see [`emit_at`].
#[cfg(feature = "log-usb")]
fn emit(level: Level, fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize) {
    // The clock is read BEFORE the lock: it keeps the timer driver's own locking out of
    // the IRQ-off window, and a millisecond of skew is invisible in a millisecond field.
    // Everything that must agree with wire order — seq, frame, space check, commit — is
    // inside.
    let now_ms = embassy_time::Instant::now().as_millis() as u32;
    emit_at(level, now_ms, fill);
}

/// Whether a record may take the last of the ring, and what a refusal costs.
#[cfg(feature = "log-usb")]
#[derive(Clone, Copy)]
enum Policy {
    /// An ordinary record: it takes any space that fits, always spends a `seq`, and a refusal for
    /// space is a drop counted into `dropped_full`/`bytes_dropped`.
    Log,
    /// Bulk dump payload: it must fit around the reserve [`asperitas_wire::dump::dump_fits`]
    /// keeps for ordinary records. A refusal is asked before anything is spent, so it takes no
    /// `seq` and counts no drop - the writer retries, and a retry is not a lost record.
    Dump,
}

/// Fill, frame and commit one record - whole, or not at all - stamped `now_ms`.
///
/// The one place a record reaches [`LOG_PIPE`]. `fill` writes the body into the guarded window and
/// returns its length; the level, sequence number, checksum, and delivery are this function's
/// business. Returns whether the frame reached the pipe.
///
/// # Why each step sits where it does
///
/// - **The clock is read before the lock** by the caller, keeping the timer driver's own locking
///   out of the IRQ-off window. A millisecond of skew is invisible in a millisecond field;
///   sequence order is not, so `seq` is taken inside.
/// - **The lock is not optional.** Records are emitted from arbitrary context including the audio
///   callback, so an interrupt can preempt a producer that does not hold `RECORD_BUFS`, and two
///   interleaved `write_whole` calls splice two frames into the ring. While the lock is held the
///   consumer does *not* run: `RECORD_BUFS` is a `CriticalSectionRawMutex`, which masks
///   interrupts globally, so `Console::run()`'s drain task cannot move a byte and free capacity is
///   *frozen* for the duration. That is what makes the capacity check sound rather than
///   optimistic. Nothing inside can panic: `write_whole` reports a sink that stopped accepting
///   bytes as `frame::WriteOutcome::Stalled`, and `commit_records` crashes on that only once the
///   lock has released. The counters are the exception by design - they are the wire ledger, so
///   they are taken in the same IRQ-off window as the write they describe.
/// - **`Policy::Dump` consults `dump_fits` before `take_seq`.** Refusals therefore consume no
///   sequence number, by construction rather than by discipline: a retry loop cannot manufacture
///   `seq` gaps that a host would read as loss.
/// - **Committed records of either policy consume `seq` and bump `records_sent`.** They really
///   occupy the wire, and `seq` continuity must keep meaning loss.
/// - **`Pipe::write` / `Pipe::write_all` are not used.** Either strands a partial frame in the
///   ring whenever capacity falls short of the frame, and the pipe has one shared `write_waker`
///   woken only on the full to non-full transition, so a stranded prefix wakes nobody.
///
/// `fill` runs **inside** the record lock, so a `Display` impl that panics masks `PRIMASK` just as
/// permanently as an assert here would. `#[no_panic]` cannot see that, because it lives in
/// whatever the caller closes over; today every `fill` in the tree formats a fixed-shape message.
///
/// # What the critical section costs, bounded rather than measured
///
/// All of this runs with interrupts disabled while a 48 kHz audio block arrives every
/// ~667 us on the same single-threaded executor. The work is fixed-size:
///
/// | step | bound |
/// |---|---|
/// | format into the body window | <= 256 bytes copied |
/// | sanitise + copy into the frame | <= 200 bytes in, <= 228 bytes out |
/// | CRC-16 over the CRC-covered range | <= 220 bytes x 8 bit iterations = 1 760 shift-and-xor steps |
/// | commit | <= 2 `try_write` calls (one, plus one to cross a ring wrap) |
///
/// The table-less CRC dominates the region by a wide margin. This crate cannot measure any of it -
/// the time driver ticks at 32 768 Hz, so one tick (~30.5 us) is coarser than the window worth
/// measuring - which is why the bound is stated in bytes and iterations instead of microseconds.
/// If this region ever shows up in the audio budget, the fix is a reserve/commit ring replacing
/// the `Pipe`, not a quiet redesign of the atomicity rule.
///
/// # What CI can and cannot reach
///
/// `cargo test --workspace` builds this crate without `log-usb`, and on the host no
/// `critical-section` implementation is registered, so this function fails to *link* there. CI
/// proves the predicate ([`asperitas_wire::dump::dump_fits`], exhaustively, against a real
/// `embassy_sync` ring: see `tests/console_dump.rs`). Only the firmware release build
/// (`cd firmware && cargo build --release --features seed3`) proves the arrangement type-checks,
/// and only the rig on hardware proves runtime behaviour.
#[cfg(feature = "log-usb")]
#[deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
fn commit(
    level: Level,
    now_ms: u32,
    policy: Policy,
    fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize,
) -> bool {
    commit_records(|bufs| {
        let body_len = fill(&mut bufs.body);
        let body = bufs.body.get(..body_len).unwrap_or_default();

        if matches!(policy, Policy::Dump)
            && !asperitas_wire::dump::dump_fits(body.len(), LOG_PIPE.free_capacity())
        {
            return frame::WriteOutcome::RefusedForSpace;
        }

        let seq = console::CONSOLE.take_seq();
        let encoded = frame::encode(level, seq, now_ms, body, &mut bufs.frame);
        if encoded.truncated {
            console::CONSOLE.body_shortened();
        }

        write_and_count(bufs.frame.get(..encoded.len).unwrap_or_default())
    })
}

/// Format, frame, and commit one ordinary record stamped `now_ms`; see `commit`.
#[cfg(feature = "log-usb")]
fn emit_at(
    level: Level,
    now_ms: u32,
    fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize,
) -> bool {
    // The verdict is this function's return value: `BOOT`, `STATUS` and `log` callers discard it
    // and read loss from the console counters, while [`emit_record`] hands it back to a caller that
    // retries or reports.
    commit(level, now_ms, Policy::Log, fill)
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

    let mut w = asperitas_wire::body::BodyWriter::new(out);
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
/// The only route by which dump traffic reaches `LOG_PIPE`, and the only caller of
/// `Policy::Dump`; the lock rules and headroom ordering are explained on `commit`. Synchronous and
/// non-blocking: it either commits or returns `false` having written nothing, no `seq` and no
/// drop count, so a caller on a timer can retry it without risking a deadlock in whatever context
/// it runs.
///
/// A body longer than [`frame::MAX_BODY`] is refused outright rather than shipped shortened:
/// a chunk whose tail silently vanished reassembles into audio that measures wrong, and the
/// block CRC catching it afterwards is not the same thing as not sending it. Every builder in
/// [`asperitas_wire::dump`] writes into a `[u8; frame::MAX_BODY]` window, so reaching this branch is a caller
/// bug - hence the debug assertion alongside the refusal. The check runs *before* the record
/// lock is taken, so that assertion cannot fire with interrupts masked.
#[cfg(feature = "log-usb")]
pub fn try_emit_dump(body: &[u8]) -> bool {
    let now_ms = embassy_time::Instant::now().as_millis() as u32;

    if body.len() > frame::MAX_BODY {
        debug_assert!(
            false,
            "dump body of {} bytes exceeds MAX_BODY; refusing rather than shipping a shortened chunk",
            body.len(),
        );
        return false;
    }

    commit(Level::Info, now_ms, Policy::Dump, |out| {
        let n = body.len().min(out.len());
        out.get_mut(..n)
            .unwrap_or_default()
            .copy_from_slice(body.get(..n).unwrap_or_default());
        n
    })
}

// ---------------------------------------------------------------------------
// Bring-up: one entry point in every configuration
// ---------------------------------------------------------------------------

/// The console's long-running half, returned by [`init`].
///
/// Poll [`Console::run`] for the life of the program, beside the LED blink. Under `log-usb` it
/// drives USB enumeration, drains the log pipe to the host and reads inbound records; in every
/// other configuration there is nothing to drive (RTT is read out of RAM by the probe) and it
/// never completes, so a binary is written once rather than once per transport.
pub struct Console {
    #[cfg(feature = "log-usb")]
    usb: usb::UsbConsole,
}

impl Console {
    /// Run the console forever.
    pub async fn run(self) {
        #[cfg(feature = "log-usb")]
        self.usb.run().await;
        #[cfg(not(feature = "log-usb"))]
        core::future::pending::<()>().await;
    }
}

/// Bring up logging for whichever transport this build has, and return the [`Console`] to run.
///
/// Call once, after the HAL is initialised. `irqs` is the `bind_interrupts!` struct that binds
/// `OTG_FS` to `embassy_stm32::usb::InterruptHandler<USB_OTG_FS>`; only the USB console uses it.
///
/// - `log-usb`: builds the CDC-ACM device, installs the logger, points it at the framed
///   console and emits `BOOT`. USB peripherals are taken with `steal()`, so the caller must
///   discard `DaisyBoard::usb_peripherals` unread.
/// - `log-defmt` alone: installs the logger over RTT, which needs no device.
/// - neither: installs the logger over a no-op backend and every record is dropped.
///
/// # Panics
///
/// Under `log-usb`, panics if called more than once.
#[cfg(feature = "log-usb")]
pub fn init<I>(irqs: I) -> Console
where
    I: embassy_stm32::interrupt::typelevel::Binding<
            <embassy_stm32::peripherals::USB_OTG_FS as embassy_stm32::usb::Instance>::Interrupt,
            embassy_stm32::usb::InterruptHandler<embassy_stm32::peripherals::USB_OTG_FS>,
        > + 'static,
{
    let usb = usb::init(irqs);
    // The order is load-bearing: `BOOT` before the backend switch reaches `Backend::NoOp` and
    // is discarded without a trace, and the counters cannot report a record that was never
    // offered to them.
    install_logger();
    set_backend(Backend::Usb);
    emit_boot();
    Console { usb }
}

/// See the `log-usb` variant above; without it `irqs` is unused.
#[cfg(not(feature = "log-usb"))]
pub fn init<I>(_irqs: I) -> Console {
    #[cfg(feature = "log-defmt")]
    set_backend(Backend::Defmt);
    install_logger();
    Console {}
}

/// Define the defmt global logger as a no-op.
///
/// A defmt logger supplies `_defmt_write`, `_defmt_acquire`, `_defmt_release` and `_defmt_flush`,
/// which every defmt frame inside embassy-stm32 and daisy-embassy resolves against, so a binary
/// needs exactly one even when it logs nothing through defmt. Without a probe nobody scans RAM
/// for RTT, so this one discards every byte. The invoking crate must depend on `defmt`.
#[macro_export]
macro_rules! noop_defmt_logger {
    () => {
        #[defmt::global_logger]
        struct Logger;

        unsafe impl defmt::Logger for Logger {
            fn acquire() {}
            unsafe fn release() {}
            unsafe fn flush() {}
            unsafe fn write(data: &[u8]) {
                let _ = data;
            }
        }
    };
}
