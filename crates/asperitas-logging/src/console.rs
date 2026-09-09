//! Device-side console state: sequence numbers, cumulative loss counters, and the two
//! record bodies whose bytes are a wire contract (`BOOT`, `STATUS`).
//!
//! # Why this module is not behind `log-usb`
//!
//! Everything here is plain arithmetic on `AtomicU32` and `core::fmt` — no critical
//! section, no clock, no pipe — so `cargo test --workspace` compiles and tests it on the
//! host even though the root workspace never enables `log-usb`. That matters because
//! [`status_body`] and [`StatusGate::due`] *are* the protocol: TASK-030.03 documents the
//! field set and TASK-031 parses it. A change to either should fail a host test in CI,
//! not first show up on a board three time zones away.
//!
//! The commit path that *uses* this lives in `lib.rs`; the drain task that reports it
//! lives in `usb.rs`.
//!
//! # Counter semantics (TASK-030 §5, amendment A3)
//!
//! Counters are cumulative-since-boot and the host recovers rates by differencing
//! successive `STATUS` records. A counter that wrapped would turn into an enormous
//! negative rate and read as a decoder bug, so every counter **saturates** at
//! `u32::MAX`: 24 days of sustained dropping at ring capacity is reachable on a rig left
//! running, and the reading must stay truthful rather than roll over.
//!
//! `seq` deliberately does the opposite and wraps mod 2^32, because the wire field is
//! exactly 8 hex digits: a saturated sequence number would emit the *same* `seq` twice
//! and invent a record, while a wrapped one is recoverable by the reader with modular
//! subtraction (`seq_now − seq_prev` as `u32`; a jump ≥ 2^31 means restart or corruption,
//! not loss). The asymmetry is the point — a counter describes an amount, a sequence
//! number describes a position.

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::TruncWriter;

/// How many bytes of body a producer may format into.
///
/// Wider than [`crate::frame::MAX_BODY`] (200) on purpose: the encoder decides whether a
/// body was shortened, and it decides from the *input* length. A formatter capped at
/// exactly 200 would report `truncated == false` forever and the `trunc` counter could
/// never fire. Formatting therefore gets a 256-byte window and the encoder caps it to
/// 200 while setting the flag.
pub const BODY_WINDOW: usize = 256;

/// Minimum spacing between `STATUS` records, in milliseconds.
const STATUS_MIN_INTERVAL_MS: u32 = 1000;

// ---------------------------------------------------------------------------
// Cumulative counters
// ---------------------------------------------------------------------------

/// One consistent read of every console counter, including the next sequence number.
///
/// `Copy` + `PartialEq` so [`StatusGate`] can compare snapshots with `!=` instead of
/// field-by-field bookkeeping that a later field addition would silently skip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleCounters {
    /// Records committed to the log pipe whole.
    pub records_sent: u32,
    /// Records refused by the pipe for lack of space — never partially written.
    pub dropped_full: u32,
    /// Bytes those refused records would have occupied.
    pub bytes_dropped: u32,
    /// Bodies shortened by the [`crate::frame::MAX_BODY`] cap. Those records shipped.
    pub truncated: u32,
    /// Endpoint writes that failed, i.e. link loss or stalls.
    pub endpoint_errors: u32,
    /// Value the next record will carry as its `seq`.
    pub seq_next: u32,
}

/// Cumulative console state for one boot: the sequence number producers hand out and the
/// counters `STATUS` reports.
///
/// `Relaxed` throughout. This is a single-core M7 and these exist to be snapshotted
/// without touching the record lock, not to order anything else.
#[derive(Debug)]
pub struct ConsoleStats {
    seq: AtomicU32,
    records_sent: AtomicU32,
    dropped_full: AtomicU32,
    bytes_dropped: AtomicU32,
    truncated: AtomicU32,
    endpoint_errors: AtomicU32,
}

/// The device's console state. Producers use it inside the record lock; the drain task
/// reads it outside any lock.
pub static CONSOLE: ConsoleStats = ConsoleStats::new();

impl Default for ConsoleStats {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleStats {
    /// Zeroed state, as at boot.
    pub const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            records_sent: AtomicU32::new(0),
            dropped_full: AtomicU32::new(0),
            bytes_dropped: AtomicU32::new(0),
            truncated: AtomicU32::new(0),
            endpoint_errors: AtomicU32::new(0),
        }
    }

    /// Take the sequence number for the next record.
    ///
    /// Wraps mod 2^32 — see the module docs. Callers assign it **inside** the record
    /// lock, which is what makes numeric order equal wire order.
    pub fn take_seq(&self) -> u32 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// A record reached the pipe whole.
    pub fn record_committed(&self) {
        bump(&self.records_sent);
    }

    /// A record was refused because it did not fit. `bytes` is the size of the frame that
    /// never went out, so the ring is untouched and only these two counters move.
    pub fn record_dropped_for_space(&self, bytes: usize) {
        bump(&self.dropped_full);
        add(
            &self.bytes_dropped,
            u32::try_from(bytes).unwrap_or(u32::MAX),
        );
    }

    /// A body exceeded [`crate::frame::MAX_BODY`] and shipped shortened.
    pub fn body_shortened(&self) {
        bump(&self.truncated);
    }

    /// An endpoint write failed.
    pub fn endpoint_error(&self) {
        bump(&self.endpoint_errors);
    }

    /// Snapshot every counter in one place, so no caller hand-rolls six loads and gets
    /// the set out of date when a counter is added.
    pub fn snapshot(&self) -> ConsoleCounters {
        ConsoleCounters {
            records_sent: self.records_sent.load(Ordering::Relaxed),
            dropped_full: self.dropped_full.load(Ordering::Relaxed),
            bytes_dropped: self.bytes_dropped.load(Ordering::Relaxed),
            truncated: self.truncated.load(Ordering::Relaxed),
            endpoint_errors: self.endpoint_errors.load(Ordering::Relaxed),
            seq_next: self.seq.load(Ordering::Relaxed),
        }
    }
}

/// Add one, stopping at `u32::MAX` rather than rolling over.
fn bump(counter: &AtomicU32) {
    add(counter, 1);
}

/// Add `n`, saturating at `u32::MAX` (see the module docs for why).
fn add(counter: &AtomicU32, n: u32) {
    // `fetch_update` returns `Err` once the closure says "no change", i.e. once we are
    // saturated — which is exactly the state we want to sit in.
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(n));
}

// ---------------------------------------------------------------------------
// Record bodies that are wire format
// ---------------------------------------------------------------------------

/// Render the `BOOT` banner into `out`, returning the bytes written.
///
/// Field order is normative (TASK-030 §3): `proto` first so a reader can handshake
/// before trusting anything, then what produced the capture. `fw_version` is the
/// `asperitas-logging` crate version, which is the version that ships the console — the
/// firmware binaries have no separate release process.
pub fn boot_body(
    out: &mut [u8; BODY_WINDOW],
    fw_version: &str,
    pipe_bytes: usize,
    max_body: usize,
) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "BOOT proto=1 fw={} pipe={} maxbody={}",
        fw_version,
        pipe_bytes,
        max_body
    );
    w.filled()
}

/// Render a `STATUS` record body into `out`, returning the bytes written.
///
/// The field set and their order are the contract TASK-030.03 documents and TASK-031
/// parses; the unit test below fails if either changes. Values are absolute
/// since-boot counts, never per-interval deltas — the host owns the differencing.
///
/// `pipe_free` is the ring's free bytes at the instant of rendering, which is what makes
/// a drop storm diagnosable after the fact: it shows how close to full the ring was
/// rather than merely that something was dropped.
pub fn status_body(out: &mut [u8; BODY_WINDOW], snap: &ConsoleCounters, pipe_free: usize) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "STATUS proto=1 sent={} dropped_full={} bytes_dropped={} trunc={} ep_err={} seq_next={} pipe_free={}",
        snap.records_sent,
        snap.dropped_full,
        snap.bytes_dropped,
        snap.truncated,
        snap.endpoint_errors,
        snap.seq_next,
        pipe_free
    );
    w.filled()
}

// ---------------------------------------------------------------------------
// STATUS pacing
// ---------------------------------------------------------------------------

/// Decides when a `STATUS` record is owed: at most one per second, and only when a
/// counter moved.
///
/// Debouncing is not politeness. During a full-ring condition the `STATUS` record competes
/// for the very space that is missing, so an undebounced emitter starves real logs of the
/// bytes it is reporting on. Riding the drain task also means no timer and no second
/// task — spawning one would force a change to every firmware call site.
#[derive(Default, Clone, Copy)]
pub struct StatusGate {
    /// When we last *attempted* an emission, whether or not the record survived.
    last_attempt_ms: Option<u32>,
    /// The counter set as of the last emission, used for the "something moved" half.
    last_seen: Option<ConsoleCounters>,
}

impl StatusGate {
    pub const fn new() -> Self {
        Self {
            last_attempt_ms: None,
            last_seen: None,
        }
    }

    /// Whether a `STATUS` record is due at `now_ms`, arming the ≥1 s window when it says yes.
    ///
    /// Arming happens at the decision, not after a successful write: a `STATUS` record
    /// lost to a full ring must cost one attempt per second, not one attempt per drain-loop
    /// iteration. The honest limit worth knowing — a lost `STATUS` is indistinguishable
    /// from nothing having happened, except through the `seq` gap it leaves behind.
    ///
    /// `now_ms` is `u32` milliseconds and wraps every ~49.7 days; the comparison is
    /// `wrapping_sub`, which stays correct across that wrap.
    pub fn due(&mut self, now_ms: u32, snap: &ConsoleCounters) -> bool {
        let changed = match self.last_seen {
            Some(prev) => prev != *snap,
            // Nothing has been reported yet, so the current state is itself news.
            None => true,
        };
        let paced = match self.last_attempt_ms {
            Some(prev) => now_ms.wrapping_sub(prev) >= STATUS_MIN_INTERVAL_MS,
            None => true,
        };

        if changed && paced {
            self.last_attempt_ms = Some(now_ms);
            true
        } else {
            false
        }
    }

    /// Note that a `STATUS` record was built and handed to the commit path, recording the
    /// counter set read *afterwards*.
    ///
    /// Reading after the emission is what stops a quiet device from talking forever:
    /// emitting `STATUS` bumps `records_sent` and `seq_next` itself, so remembering the
    /// pre-emission snapshot would leave a permanently-changed counter and earn one
    /// record per second from a board with nothing to say. Anything another producer
    /// bumped in the meantime is folded in here too and simply rides the next `STATUS`
    /// that has a real reason to exist.
    pub fn mark_sent(&mut self, snap_after: &ConsoleCounters) {
        self.last_seen = Some(*snap_after);
    }
}

// ---------------------------------------------------------------------------
// Host tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(
        sent: u32,
        dropped_full: u32,
        bytes_dropped: u32,
        truncated: u32,
        endpoint_errors: u32,
        seq_next: u32,
    ) -> ConsoleCounters {
        ConsoleCounters {
            records_sent: sent,
            dropped_full,
            bytes_dropped,
            truncated,
            endpoint_errors,
            seq_next,
        }
    }

    #[test]
    fn status_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let n = status_body(&mut out, &counters(12, 3, 4_096, 1, 2, 18), 2_048);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "STATUS proto=1 sent=12 dropped_full=3 bytes_dropped=4096 trunc=1 ep_err=2 seq_next=18 pipe_free=2048"
        );
    }

    #[test]
    fn status_body_renders_saturated_counters_as_u32_max() {
        let mut out = [0u8; BODY_WINDOW];
        let maxed = counters(u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        let n = status_body(&mut out, &maxed, usize::MAX);
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(
            text.starts_with("STATUS proto=1 sent=4294967295 dropped_full=4294967295 "),
            "{text}"
        );
        assert!(text.contains("seq_next=4294967295 pipe_free="), "{text}");
        // Worst-case rendering must still leave room inside the body window, or a
        // saturated device starts truncating the record that reports its losses.
        assert!(
            n < crate::frame::MAX_BODY,
            "worst-case STATUS body is {n} bytes, cap is {}",
            crate::frame::MAX_BODY
        );
    }

    #[test]
    fn boot_body_announces_the_console_geometry() {
        let mut out = [0u8; BODY_WINDOW];
        let n = boot_body(&mut out, "0.1.0", 2048, crate::frame::MAX_BODY);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "BOOT proto=1 fw=0.1.0 pipe=2048 maxbody=200"
        );
    }

    /// Both synthetic bodies must be legal v1 payloads, not merely plausible text: a
    /// `STATUS` record that tripped the encoder's cap or contained something the sanitiser
    /// mangled would report counters while failing its own checksum.
    #[test]
    fn boot_and_status_bodies_survive_the_wire_codec() {
        use crate::frame::{encode, Decoder, MAX_FRAME};

        let mut boot_buf = [0u8; BODY_WINDOW];
        let boot_len = boot_body(&mut boot_buf, "0.1.0", 2048, crate::frame::MAX_BODY);

        let maxed = counters(u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        let mut status_buf = [0u8; BODY_WINDOW];
        let status_len = status_body(&mut status_buf, &maxed, 2_048);

        for (window, len) in [(&boot_buf, boot_len), (&status_buf, status_len)] {
            let body = &window[..len];
            let mut frame_buf = [0u8; MAX_FRAME];
            let enc = encode(log::Level::Info, 7, 1_234, body, &mut frame_buf);
            assert!(
                !enc.truncated,
                "console body of {} bytes was capped",
                body.len()
            );

            let mut decoder = Decoder::new();
            decoder.push(&frame_buf[..enc.len]);
            let record = decoder.next_record().expect("record must decode");
            assert_eq!(record.body, body);
            assert_eq!(record.seq, 7);
            assert_eq!(record.t_ms, 1_234);
            let stats = decoder.stats();
            assert_eq!(stats.bad_frames, 0, "{stats:?}");
            assert_eq!(stats.records, 1, "{stats:?}");
        }
    }

    #[test]
    fn counters_saturate_instead_of_wrapping() {
        let stats = ConsoleStats::new();
        // One refusal larger than `u32::MAX` bytes cannot happen, but it is the cheapest
        // way to land on the ceiling: `bump` and `add` share the same checked path.
        stats.record_dropped_for_space(usize::MAX);
        let snap = stats.snapshot();
        assert_eq!(snap.dropped_full, 1);
        assert_eq!(snap.bytes_dropped, u32::MAX, "must saturate, not wrap");

        stats.record_dropped_for_space(10);
        let snap = stats.snapshot();
        assert_eq!(snap.bytes_dropped, u32::MAX, "stays pinned at the ceiling");
        assert_eq!(snap.dropped_full, 2, "the count keeps its own truth");
    }

    #[test]
    fn sequence_numbers_are_monotonic_and_wrap_is_plain_modular() {
        let stats = ConsoleStats::new();
        assert_eq!(stats.take_seq(), 0);
        assert_eq!(stats.take_seq(), 1);
        assert_eq!(stats.snapshot().seq_next, 2);
        // `fetch_add` on a `u32` is defined to wrap mod 2^32, which is the behaviour the
        // 8-hex-digit wire field assumes; the reader recovers with modular subtraction.
        assert_eq!(u32::MAX.wrapping_add(1), 0);
    }

    #[test]
    fn gate_emits_once_then_waits_a_second() {
        let mut gate = StatusGate::new();
        let a = counters(1, 0, 0, 0, 0, 1);

        assert!(gate.due(10, &a), "first report is news");
        gate.mark_sent(&a);

        let b = counters(2, 0, 0, 0, 0, 2);
        assert!(
            !gate.due(500, &b),
            "sub-second is suppressed even when counters moved"
        );
        assert!(gate.due(1_010, &b), "one counter moved after a second");
        gate.mark_sent(&b);
    }

    #[test]
    fn gate_stays_quiet_when_nothing_moved() {
        let mut gate = StatusGate::new();
        let a = counters(1, 0, 0, 0, 0, 1);
        assert!(gate.due(0, &a));
        gate.mark_sent(&a);

        assert!(!gate.due(5_000, &a), "an idle device owes no STATUS");
        assert!(!gate.due(60_000, &a));
    }

    #[test]
    fn gate_attempts_once_per_second_even_when_every_attempt_is_dropped() {
        let mut gate = StatusGate::new();
        let a = counters(1, 0, 0, 0, 0, 1);
        assert!(gate.due(0, &a));
        // Deliberately no `mark_sent`: the record never made it onto the wire.
        assert!(
            !gate.due(100, &a),
            "a dropped STATUS must not become a retry storm"
        );
        assert!(!gate.due(999, &a));
        assert!(
            gate.due(1_000, &a),
            "and the next second is fair game again"
        );
    }

    #[test]
    fn gate_pacing_survives_the_clock_wrapping_at_2_pow_32_ms() {
        let mut gate = StatusGate::new();
        let a = counters(1, 0, 0, 0, 0, 1);
        let near_wrap = u32::MAX - 500;
        assert!(gate.due(near_wrap, &a));
        gate.mark_sent(&a);

        let b = counters(2, 0, 0, 0, 0, 2);
        // 400 ms after the wrap: only 900 ms of real time have passed.
        assert!(
            !gate.due(399, &b),
            "wrap must not manufacture a free emission"
        );
        // Past the wrap by 1000 ms of real time.
        assert!(gate.due(500, &b));
    }
}
