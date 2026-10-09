//! Device-side console state: sequence numbers, cumulative loss counters, and the record
//! bodies whose bytes are a wire contract (`BOOT`, `STATUS`, and the six measurement-rig
//! verbs `RIGCFG`, `RIGGEN`, `CAPSTAT`, `CAPMAX`, `STIMSTART`, `DUMPEND`).
//!
//! # Why this module is not behind `log-usb`
//!
//! Everything here is plain arithmetic on `AtomicU32` and `core::fmt` — no critical
//! section, no clock, no pipe — so `cargo test --workspace` compiles and tests it on the
//! host even though the root workspace never enables `log-usb`, for the same reason
//! [`crate::frame`] is ungated. That matters because
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
// Measurement-rig verbs (TASK-038.03.02)
// ---------------------------------------------------------------------------

/// How many decimal digits a saturated `u32` renders as: `4294967295`.
///
/// Every numeric field in the field tables below is a `u32` and therefore costs exactly this
/// many bytes at its worst. Naming the number is what makes those tables auditable: a table
/// that said `11` would pass the same asserts while quietly overstating the verb, and one that
/// said `9` would understate it and let a real device overflow one record.
const U32_MAX_DIGITS: usize = 10;

/// Worst-case length of a body: its literal prefix, plus `1 + name + 1 + width` per field.
///
/// This is the arithmetic the [`TruncWriter`]-based builders above cannot express in types - a
/// format string says nothing about how long its result gets. Computing the sum here and
/// asserting against [`crate::frame::MAX_BODY`] moves *"this verb fits one record"* from a
/// nightly test run to the build itself, so a field added to a table fails compilation rather
/// than shipping a body that silently truncates at 200 bytes. The runtime saturated tests
/// beside each builder then prove the *builder* still renders what its table claims, by
/// asserting the two lengths are equal.
const fn saturated_len(prefix: &str, fields: &[(&str, usize)]) -> usize {
    let mut total = prefix.len();
    let mut i = 0;
    while i < fields.len() {
        let (name, width) = fields[i];
        total += 1 + name.len() + 1 + width;
        i += 1;
    }
    total
}

/// Which channel the rig captured, as `RIGCFG`'s `lane` field.
///
/// A named type rather than a `bool` or a `char` because the field set is a contract: an
/// argument of either of those shapes lets a caller put garbage on the wire, and a host parser
/// that sees `lane=x` has no way to tell that from a real reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonoLane {
    /// The left, or loop, channel.
    Left,
    /// The right channel.
    Right,
}

impl MonoLane {
    /// The one-character wire spelling of this lane.
    const fn letter(self) -> &'static str {
        match self {
            MonoLane::Left => "L",
            MonoLane::Right => "R",
        }
    }
}

/// What the measurement rig was configured to do, as one `RIGCFG` record.
///
/// Sent once at boot, before any stimulus plays: everything a host needs to interpret the
/// captured bytes that follow. Numeric fields stay `u32` for uniformity with `STATUS` even
/// where the source value is wider: `capture`'s byte totals come back as `usize` and
/// [`crate::capture::ring_duration_micros`] as a `u64`. Narrowing is safe at the call site today
/// (both values fit, and `rig.rs` narrows explicitly) and is *not* a claim that a ring can never
/// grow past what these fields hold - a ring twice today's size would need them widened, which is
/// a protocol change and will read as one.
#[derive(Clone, Copy, Debug)]
pub struct RigConfig {
    /// Channel captured into the mono stream.
    pub lane: MonoLane,
    /// Ring blocks the capture expects to fill.
    pub blocks: u32,
    /// Raw payload bytes per ring block.
    pub block_bytes: u32,
    /// Captured bytes per second of audio.
    pub bytes_per_s: u32,
    /// Exact ring capacity in microseconds.
    pub capsec_us: u32,
    /// Capture window in seconds.
    pub window_s: u32,
    /// Measured CPU clock in Hz - read from the device, never declared (§3 of the parent plan).
    pub cpu_hz: u32,
    /// Whether the instruction cache is enabled, as reported by the hardware.
    pub icache: bool,
    /// Whether the data cache is enabled, as reported by the hardware.
    pub dcache: bool,
}

/// Field table behind [`rigcfg_body`]: `(name, worst-case width)` in wire order.
const RIGCFG_FIELDS: [(&str, usize); 9] = [
    ("lane", 1),
    ("blocks", U32_MAX_DIGITS),
    ("block_bytes", U32_MAX_DIGITS),
    ("bytes_per_s", U32_MAX_DIGITS),
    ("capsec_us", U32_MAX_DIGITS),
    ("window_s", U32_MAX_DIGITS),
    ("cpu_hz", U32_MAX_DIGITS),
    ("icache", 1),
    ("dcache", 1),
];

/// Worst-case `RIGCFG` body: 177 bytes, inside [`crate::frame::MAX_BODY`] with 23 to spare.
const RIGCFG_WORST: usize = saturated_len("RIGCFG proto=1 capture=mono16", &RIGCFG_FIELDS);

/// Render a `RIGCFG` record body into `out`, returning the bytes written.
///
/// Carries numbers only. The generator's free-text description lives in its own
/// [`riggen_body`] record, because a body mixing one unbounded string with nine numbers has no
/// checkable bound: `RIGCFG`'s numeric fields leave 23 bytes of slack, less than the default
/// pulse-train description (97), so any string that could actually be sent would blow the
/// record. Splitting them keeps both verbs provably one-record.
pub fn rigcfg_body(cfg: &RigConfig, out: &mut [u8; BODY_WINDOW]) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "RIGCFG proto=1 capture=mono16 lane={} blocks={} block_bytes={} bytes_per_s={} capsec_us={} window_s={} cpu_hz={} icache={} dcache={}",
        cfg.lane.letter(),
        cfg.blocks,
        cfg.block_bytes,
        cfg.bytes_per_s,
        cfg.capsec_us,
        cfg.window_s,
        cfg.cpu_hz,
        u8::from(cfg.icache),
        u8::from(cfg.dcache),
    );
    w.filled()
}

/// Most generator-description bytes [`riggen_body`] will carry: 160.
///
/// Named so the caller can check its own payload against the budget instead of trusting a
/// comment. `asperitas-dsp` guarantees only that `describe()` stays under `frame::MAX_BODY`,
/// which is 16 bytes more than this verb can carry; `rig.rs` debug-asserts the length
/// `describe()` actually returned against this constant.
pub const RIGGEN_MAX_GEN_BYTES: usize = 160;

/// Worst-case `RIGGEN` body: the 15-byte prefix plus [`RIGGEN_MAX_GEN_BYTES`] = 175.
const RIGGEN_WORST: usize = "RIGGEN proto=1 ".len() + RIGGEN_MAX_GEN_BYTES;

/// Render a `RIGGEN` record body into `out`: the prefix, then the generator's own
/// description verbatim, returning the bytes written.
///
/// The payload goes last and is clipped to [`RIGGEN_MAX_GEN_BYTES`], so a description that
/// overruns loses the tail of its *last* field rather than shifting or mangling a number the
/// host parses. Clipping is silent for the same reason [`status_body`] truncates: this runs on
/// a path that must not panic.
///
/// Unlike its siblings this one does not go through `TruncWriter`. Its payload is `&[u8]`,
/// and `fmt::Write` only accepts `&str`: converting would mean either panicking on invalid
/// UTF-8 or replacing bytes the device actually measured, and `describe()`'s output is plain
/// ASCII anyway, so neither failure mode is worth paying for. The bytes are copied, not
/// interpreted.
pub fn riggen_body(describe: &[u8], out: &mut [u8; BODY_WINDOW]) -> usize {
    const HEADER: &[u8] = b"RIGGEN proto=1 ";
    let text = &describe[..describe.len().min(RIGGEN_MAX_GEN_BYTES)];
    let take = text.len().min(out.len() - HEADER.len());
    out[..HEADER.len()].copy_from_slice(HEADER);
    out[HEADER.len()..HEADER.len() + take].copy_from_slice(&text[..take]);
    HEADER.len() + take
}

/// What the capture producer has done so far, as one `CAPSTAT` record.
///
/// Deliberately missing three fields a first draft carried, each because the rule *a fact
/// appears on the wire once* puts it elsewhere: `sent` and `bytes_dropped` belong to `STATUS`
/// (which also carries `seq_next`, so an interval is still bracketable), and `free` is
/// derivable from [`crate::capture::RING_BLOCKS`] plus two fields already here.
#[derive(Clone, Copy, Debug)]
pub struct CaptureStatus {
    /// Blocks handed to the ring since boot.
    pub delivered: u32,
    /// Blocks the window still expects, counting down.
    pub expected: u32,
    /// Blocks the producer had to discard because the ring was full.
    pub overrun: u32,
    /// Longest single audio callback seen, in microseconds.
    pub max_block_us: u32,
    /// Longest gap between callbacks seen, in microseconds.
    pub worst_gap_us: u32,
    /// Audio engine health: 0 running, 1 `start_interface` failed, 2 `start_callback` failed.
    ///
    /// Its own counter because a dead engine and a starved one otherwise share a symptom:
    /// `delivered` simply stops moving either way, and the stream cannot tell them apart.
    pub audio_exit: u32,
    /// Blocks whose `AUDEND` has been committed - dump progress as a count, not an index.
    pub dumped: u32,
    /// Records the log pipe refused for lack of space.
    pub dropped_full: u32,
}

/// Field table behind [`capstat_body`].
const CAPSTAT_FIELDS: [(&str, usize); 8] = [
    ("delivered", U32_MAX_DIGITS),
    ("expected", U32_MAX_DIGITS),
    ("overrun", U32_MAX_DIGITS),
    ("max_block_us", U32_MAX_DIGITS),
    ("worst_gap_us", U32_MAX_DIGITS),
    ("audio_exit", U32_MAX_DIGITS),
    ("dumped", U32_MAX_DIGITS),
    ("dropped_full", U32_MAX_DIGITS),
];

/// Worst-case `CAPSTAT` body: 187 bytes, 13 inside the cap.
///
/// The tightest of the four numeric verbs relative to when it repeats, which is why its rate
/// gate exists (parent plan §8) and divides by [`CAPSTAT_MAX_BODY`].
const CAPSTAT_WORST: usize = saturated_len("CAPSTAT proto=1", &CAPSTAT_FIELDS);

/// The byte budget a `CAPSTAT` record may never exceed: [`crate::frame::MAX_BODY`].
///
/// An alias rather than a second `200`, so the rate gate in `rig.rs` and the encoder cannot
/// drift apart while both claim to know the cap. It lives next to the builder that produces
/// the record and is checked by `capstat_body_saturated_counters_fit_one_frame`, because a
/// gate that divided by a hand-typed number would be arithmetic nobody tested.
pub const CAPSTAT_MAX_BODY: usize = crate::frame::MAX_BODY;

// The alias must never exceed what the encoder caps at. Checked at compile time rather than
// in the test below, because a comparison between two constants has one possible outcome and a
// runtime assertion on it can only ever pass; `clippy::assertions_on_constants` says so too.
const _: () = assert!(CAPSTAT_MAX_BODY <= crate::frame::MAX_BODY);

/// Render a `CAPSTAT` record body into `out`, returning the bytes written.
pub fn capstat_body(st: &CaptureStatus, out: &mut [u8; BODY_WINDOW]) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "CAPSTAT proto=1 delivered={} expected={} overrun={} max_block_us={} worst_gap_us={} audio_exit={} dumped={} dropped_full={}",
        st.delivered,
        st.expected,
        st.overrun,
        st.max_block_us,
        st.worst_gap_us,
        st.audio_exit,
        st.dumped,
        st.dropped_full,
    );
    w.filled()
}

/// What the capture ring could hold at all, as one `CAPMAX` record.
///
/// Reported once at boot alongside `RIGCFG`. These are the constants the device was compiled
/// with, not measurements, so a host can state the ceiling a `CAPSTAT` overrun must be judged
/// against without knowing anything about this firmware's geometry.
#[derive(Clone, Copy, Debug)]
pub struct RingCapacity {
    /// Bytes the requested window needs.
    pub total_bytes: u32,
    /// Bytes the ring can hold.
    pub ring_bytes: u32,
    /// Whole seconds the ring can hold, floored.
    pub seconds_max: u32,
    /// Exact ring capacity in microseconds.
    pub us_max: u32,
    /// Bytes the window leaves unused.
    pub unused_headroom_bytes: u32,
}

/// Field table behind [`capmax_body`].
const CAPMAX_FIELDS: [(&str, usize); 5] = [
    ("total_bytes", U32_MAX_DIGITS),
    ("ring_bytes", U32_MAX_DIGITS),
    ("seconds_max", U32_MAX_DIGITS),
    ("us_max", U32_MAX_DIGITS),
    ("unused_headroom_bytes", U32_MAX_DIGITS),
];

/// Worst-case `CAPMAX` body: 133 bytes, 67 inside the cap.
const CAPMAX_WORST: usize = saturated_len("CAPMAX proto=1", &CAPMAX_FIELDS);

/// Render a `CAPMAX` record body into `out`, returning the bytes written.
pub fn capmax_body(cap: &RingCapacity, out: &mut [u8; BODY_WINDOW]) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "CAPMAX proto=1 total_bytes={} ring_bytes={} seconds_max={} us_max={} unused_headroom_bytes={}",
        cap.total_bytes,
        cap.ring_bytes,
        cap.seconds_max,
        cap.us_max,
        cap.unused_headroom_bytes,
    );
    w.filled()
}

/// Where a one-shot stimulus started inside the capture, as one `STIMSTART` record.
///
/// Sent once, after the window closes and before its dump, and only by a build whose stimulus plays
/// once (an exponential sweep): a periodic one has no start worth locating. `offset` is the position
/// of the callback that rendered the stimulus's first sample, counted in samples from the first
/// sample of block `first_block` - the PCM `dump_reassemble` writes, from its first byte. What comes
/// back through the loop arrives later than that by the loop's own latency, which the capture
/// measures; the record says where playback began, not where the echo did.
#[derive(Clone, Copy, Debug)]
pub struct StimulusStart {
    /// Sequence number of the window's first block, as the dump's `AUDIO` records carry it.
    pub first_block: u32,
    /// Samples from that block's first sample to the stimulus's first rendered sample.
    pub offset: u32,
}

/// Field table behind [`stimstart_body`].
const STIMSTART_FIELDS: [(&str, usize); 2] =
    [("first_block", U32_MAX_DIGITS), ("offset", U32_MAX_DIGITS)];

/// Worst-case `STIMSTART` body: 59 bytes.
const STIMSTART_WORST: usize = saturated_len("STIMSTART proto=1", &STIMSTART_FIELDS);

/// Render a `STIMSTART` record body into `out`, returning the bytes written.
pub fn stimstart_body(st: &StimulusStart, out: &mut [u8; BODY_WINDOW]) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "STIMSTART proto=1 first_block={} offset={}",
        st.first_block,
        st.offset,
    );
    w.filled()
}

/// How one completed dump went, as one `DUMPEND` record.
///
/// The record about a single dump, which is why `refused` and `stall_ms` live here rather than
/// in the repeating `CAPSTAT`: they describe the transfer this record closes, not the device's
/// life so far. `sent`/`bytes_dropped` are the console's cumulative ledger, restated here for
/// the interval the dump spans.
#[derive(Clone, Copy, Debug)]
pub struct DumpSummary {
    /// Blocks transferred in this dump.
    pub blocks: u32,
    /// Chunk records those blocks cost.
    pub chunks: u32,
    /// Raw payload bytes delivered.
    pub bytes: u32,
    /// Wall time from first chunk to `AUDEND`, in milliseconds.
    pub elapsed_ms: u32,
    /// Times the headroom rule refused a chunk and the writer retried.
    pub refused: u32,
    /// Longest stretch with no forward progress, in milliseconds.
    pub stall_ms: u32,
    /// Console records sent overall.
    pub sent: u32,
    /// Console records refused for space overall.
    pub dropped_full: u32,
    /// Bytes those refusals would have occupied.
    pub bytes_dropped: u32,
}

/// Field table behind [`dumpend_body`].
const DUMPEND_FIELDS: [(&str, usize); 9] = [
    ("blocks", U32_MAX_DIGITS),
    ("chunks", U32_MAX_DIGITS),
    ("bytes", U32_MAX_DIGITS),
    ("elapsed_ms", U32_MAX_DIGITS),
    ("refused", U32_MAX_DIGITS),
    ("stall_ms", U32_MAX_DIGITS),
    ("sent", U32_MAX_DIGITS),
    ("dropped_full", U32_MAX_DIGITS),
    ("bytes_dropped", U32_MAX_DIGITS),
];

/// Worst-case `DUMPEND` body: 194 bytes, 6 inside the cap - the tightest verb on the wire.
const DUMPEND_WORST: usize = saturated_len("DUMPEND proto=1", &DUMPEND_FIELDS);

/// Render a `DUMPEND` record body into `out`, returning the bytes written.
pub fn dumpend_body(d: &DumpSummary, out: &mut [u8; BODY_WINDOW]) -> usize {
    let mut w = TruncWriter::new(out);
    let _ = core::write!(
        w,
        "DUMPEND proto=1 blocks={} chunks={} bytes={} elapsed_ms={} refused={} stall_ms={} sent={} dropped_full={} bytes_dropped={}",
        d.blocks,
        d.chunks,
        d.bytes,
        d.elapsed_ms,
        d.refused,
        d.stall_ms,
        d.sent,
        d.dropped_full,
        d.bytes_dropped,
    );
    w.filled()
}

// One record each, at their own worst case. These six lines are the half a drifted format
// string breaks and a runtime test would only catch on the night it runs.
const _: () = assert!(RIGCFG_WORST < crate::frame::MAX_BODY);
const _: () = assert!(RIGGEN_WORST < crate::frame::MAX_BODY);
const _: () = assert!(CAPSTAT_WORST < crate::frame::MAX_BODY);
const _: () = assert!(CAPMAX_WORST < crate::frame::MAX_BODY);
const _: () = assert!(STIMSTART_WORST < crate::frame::MAX_BODY);
const _: () = assert!(DUMPEND_WORST < crate::frame::MAX_BODY);

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

    /// Every body this module renders must be a legal v1 payload, not merely plausible text: a
    /// record that tripped the encoder's cap or contained something the sanitiser mangled would
    /// report counters while failing its own checksum.
    ///
    /// All eight builders go through the one loop on purpose. Each verb got its own saturated
    /// test below because a shared loop reporting "one of these was capped" hides which, but a
    /// shared loop is the right shape for "none of them breaks the codec".
    #[test]
    fn all_console_bodies_survive_the_wire_codec() {
        use crate::frame::{encode, Decoder, MAX_FRAME};

        let maxed = counters(u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        let mut bufs: [[u8; BODY_WINDOW]; 8] = [[0; BODY_WINDOW]; 8];

        let lens = [
            boot_body(&mut bufs[0], "0.1.0", 2048, crate::frame::MAX_BODY),
            status_body(&mut bufs[1], &maxed, 2_048),
            rigcfg_body(&saturated_rigcfg(), &mut bufs[2]),
            riggen_body(SINE_DESCRIBE, &mut bufs[3]),
            capstat_body(&saturated_capture_status(), &mut bufs[4]),
            capmax_body(&saturated_ring_capacity(), &mut bufs[5]),
            dumpend_body(&saturated_dump_summary(), &mut bufs[6]),
            stimstart_body(&saturated_stimulus_start(), &mut bufs[7]),
        ];

        for (window, &len) in bufs.iter().zip(&lens) {
            let body = &window[..len];
            let mut frame_buf = [0u8; MAX_FRAME];
            let enc = encode(log::Level::Info, 7, 1_234, body, &mut frame_buf);
            assert!(
                !enc.truncated,
                "console body of {} bytes was capped: {}",
                body.len(),
                core::str::from_utf8(body).unwrap_or("<invalid utf8>")
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

    // -- measurement-rig verbs ---------------------------------------------------------

    /// `asperitas-dsp`'s pinned description of a 1 kHz sine (`stimulus_tests.rs`), used here as
    /// a realistic `describe()` payload rather than a synthetic filler. If dsp renames a field,
    /// this literal is the one in two places that has to change together, which is the point.
    const SINE_DESCRIBE: &[u8] =
        b"name=sine sample_rate_hz=48000 level_dbfs=-20.0 frequency_hz=1000 period_samples=48";

    /// Every numeric field at its ceiling - the shape a long-running device reports when a
    /// counter saturates, and therefore the worst case each verb must survive.
    fn saturated_rigcfg() -> RigConfig {
        RigConfig {
            lane: MonoLane::Right,
            blocks: u32::MAX,
            block_bytes: u32::MAX,
            bytes_per_s: u32::MAX,
            capsec_us: u32::MAX,
            window_s: u32::MAX,
            cpu_hz: u32::MAX,
            icache: true,
            dcache: true,
        }
    }

    fn saturated_capture_status() -> CaptureStatus {
        CaptureStatus {
            delivered: u32::MAX,
            expected: u32::MAX,
            overrun: u32::MAX,
            max_block_us: u32::MAX,
            worst_gap_us: u32::MAX,
            audio_exit: u32::MAX,
            dumped: u32::MAX,
            dropped_full: u32::MAX,
        }
    }

    fn saturated_ring_capacity() -> RingCapacity {
        RingCapacity {
            total_bytes: u32::MAX,
            ring_bytes: u32::MAX,
            seconds_max: u32::MAX,
            us_max: u32::MAX,
            unused_headroom_bytes: u32::MAX,
        }
    }

    fn saturated_stimulus_start() -> StimulusStart {
        StimulusStart {
            first_block: u32::MAX,
            offset: u32::MAX,
        }
    }

    fn saturated_dump_summary() -> DumpSummary {
        DumpSummary {
            blocks: u32::MAX,
            chunks: u32::MAX,
            bytes: u32::MAX,
            elapsed_ms: u32::MAX,
            refused: u32::MAX,
            stall_ms: u32::MAX,
            sent: u32::MAX,
            dropped_full: u32::MAX,
            bytes_dropped: u32::MAX,
        }
    }

    #[test]
    fn rigcfg_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let cfg = RigConfig {
            lane: MonoLane::Left,
            blocks: 879,
            block_bytes: crate::capture::RING_BLOCK_BYTES as u32,
            bytes_per_s: crate::capture::BYTES_PER_SECOND as u32,
            capsec_us: crate::capture::ring_duration_micros() as u32,
            window_s: crate::capture::CAPTURE_WINDOW_SECONDS as u32,
            cpu_hz: 480_000_000,
            icache: true,
            dcache: false,
        };
        let n = rigcfg_body(&cfg, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "RIGCFG proto=1 capture=mono16 lane=L blocks=879 block_bytes=32768 bytes_per_s=96000 capsec_us=349525333 window_s=300 cpu_hz=480000000 icache=1 dcache=0"
        );
    }

    #[test]
    fn rigcfg_body_renders_saturated_fields_as_u32_max() {
        let mut out = [0u8; BODY_WINDOW];
        let n = rigcfg_body(&saturated_rigcfg(), &mut out);
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(
            text.starts_with("RIGCFG proto=1 capture=mono16 lane=R blocks=4294967295 "),
            "{text}"
        );
        assert!(text.ends_with("icache=1 dcache=1"), "{text}");
        assert!(
            n < crate::frame::MAX_BODY,
            "worst-case RIGCFG body is {n} bytes, cap is {}",
            crate::frame::MAX_BODY
        );
    }

    #[test]
    fn riggen_body_carries_the_generator_text_verbatim() {
        let mut out = [0u8; BODY_WINDOW];
        let n = riggen_body(SINE_DESCRIBE, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "RIGGEN proto=1 name=sine sample_rate_hz=48000 level_dbfs=-20.0 frequency_hz=1000 period_samples=48"
        );
    }

    /// A payload exactly at the budget must still frame whole. This is the case that makes
    /// `RIGGEN_MAX_GEN_BYTES` a promise rather than a comment: 15 + 160 = 175 < 200.
    #[test]
    fn riggen_body_at_the_full_budget_frames_untruncated() {
        use crate::frame::{encode, Decoder, MAX_FRAME};

        let describe = [b'x'; RIGGEN_MAX_GEN_BYTES];
        let mut out = [0u8; BODY_WINDOW];
        let n = riggen_body(&describe, &mut out);
        assert_eq!(n, "RIGGEN proto=1 ".len() + RIGGEN_MAX_GEN_BYTES);

        let mut frame_buf = [0u8; MAX_FRAME];
        let enc = encode(log::Level::Info, 1, 0, &out[..n], &mut frame_buf);
        assert!(
            !enc.truncated,
            "a budget-sized RIGGEN was capped at {n} bytes"
        );

        let mut decoder = Decoder::new();
        decoder.push(&frame_buf[..enc.len]);
        let record = decoder.next_record().expect("record must decode");
        assert_eq!(record.body, &out[..n]);
        assert_eq!(decoder.stats().bad_frames, 0);
    }

    /// Over-budget input clips instead of panicking or overflowing, and clipping costs only the
    /// tail of the free-text field - never a byte of the prefix a parser keys on.
    #[test]
    fn riggen_body_clips_an_overlong_description_without_panicking() {
        let describe = [b'y'; RIGGEN_MAX_GEN_BYTES + 4_096];
        let mut out = [0u8; BODY_WINDOW];
        let n = riggen_body(&describe, &mut out);
        assert_eq!(n, "RIGGEN proto=1 ".len() + RIGGEN_MAX_GEN_BYTES);
        assert!(out[..n].starts_with(b"RIGGEN proto=1 "));
        assert!(out["RIGGEN proto=1 ".len()..n].iter().all(|&b| b == b'y'));

        // And the empty case: a generator that described nothing still yields a legal record.
        let mut empty = [0u8; BODY_WINDOW];
        assert_eq!(riggen_body(&[], &mut empty), "RIGGEN proto=1 ".len());
    }

    #[test]
    fn capstat_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let st = CaptureStatus {
            delivered: 412,
            expected: 467,
            overrun: 0,
            max_block_us: 731,
            worst_gap_us: 1_102,
            audio_exit: 0,
            dumped: 100,
            dropped_full: 3,
        };
        let n = capstat_body(&st, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "CAPSTAT proto=1 delivered=412 expected=467 overrun=0 max_block_us=731 worst_gap_us=1102 audio_exit=0 dumped=100 dropped_full=3"
        );
    }

    #[test]
    fn capstat_body_saturated_counters_fit_one_frame() {
        let mut out = [0u8; BODY_WINDOW];
        let n = capstat_body(&saturated_capture_status(), &mut out);
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(
            text.starts_with("CAPSTAT proto=1 delivered=4294967295 expected=4294967295 "),
            "{text}"
        );
        assert!(
            text.ends_with("dumped=4294967295 dropped_full=4294967295"),
            "{text}"
        );

        // The bound `.02`'s rate gate divides by, checked rather than assumed: the record must
        // fit inside it, and inside what the encoder actually caps at. Whether the published
        // budget itself is sane is a compile-time question, answered by the const assert beside
        // the alias.
        assert!(
            n < CAPSTAT_MAX_BODY,
            "worst-case CAPSTAT body is {n} bytes, budget is {CAPSTAT_MAX_BODY}"
        );
        assert!(
            n < crate::frame::MAX_BODY,
            "worst-case CAPSTAT body is {n} bytes, cap is {}",
            crate::frame::MAX_BODY
        );
    }

    #[test]
    fn capmax_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let cap = RingCapacity {
            total_bytes: 28_800_000,
            ring_bytes: crate::capture::RING_BYTES as u32,
            seconds_max: crate::capture::ring_seconds_floor(),
            us_max: crate::capture::ring_duration_micros() as u32,
            unused_headroom_bytes: 4_754_432,
        };
        let n = capmax_body(&cap, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "CAPMAX proto=1 total_bytes=28800000 ring_bytes=33554432 seconds_max=349 us_max=349525333 unused_headroom_bytes=4754432"
        );
    }

    #[test]
    fn capmax_body_renders_saturated_fields_as_u32_max() {
        let mut out = [0u8; BODY_WINDOW];
        let n = capmax_body(&saturated_ring_capacity(), &mut out);
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(
            text.starts_with("CAPMAX proto=1 total_bytes=4294967295 "),
            "{text}"
        );
        assert!(text.ends_with("unused_headroom_bytes=4294967295"), "{text}");
        assert!(
            n < crate::frame::MAX_BODY,
            "worst-case CAPMAX body is {n} bytes, cap is {}",
            crate::frame::MAX_BODY
        );
    }

    #[test]
    fn stimstart_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let st = StimulusStart {
            first_block: 4,
            offset: 0,
        };
        let n = stimstart_body(&st, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "STIMSTART proto=1 first_block=4 offset=0"
        );
    }

    #[test]
    fn dumpend_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let d = DumpSummary {
            blocks: 879,
            chunks: 224_145,
            bytes: 28_800_000,
            elapsed_ms: 311_402,
            refused: 17,
            stall_ms: 940,
            sent: 4_118,
            dropped_full: 3,
            bytes_dropped: 12_288,
        };
        let n = dumpend_body(&d, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "DUMPEND proto=1 blocks=879 chunks=224145 bytes=28800000 elapsed_ms=311402 refused=17 stall_ms=940 sent=4118 dropped_full=3 bytes_dropped=12288"
        );
    }

    #[test]
    fn dumpend_body_renders_saturated_fields_as_u32_max() {
        let mut out = [0u8; BODY_WINDOW];
        let n = dumpend_body(&saturated_dump_summary(), &mut out);
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(
            text.starts_with("DUMPEND proto=1 blocks=4294967295 "),
            "{text}"
        );
        assert!(
            text.ends_with("dropped_full=4294967295 bytes_dropped=4294967295"),
            "{text}"
        );
        assert!(
            n < crate::frame::MAX_BODY,
            "worst-case DUMPEND body is {n} bytes, cap is {}",
            crate::frame::MAX_BODY
        );
    }

    /// Each builder's actual worst case must equal what its field table claims. The `const`
    /// asserts above prove the *table* fits one record; these prove the format string still
    /// renders that table, so a field added to one and not the other fails here.
    #[test]
    fn saturated_renders_match_their_field_tables() {
        let mut out = [0u8; BODY_WINDOW];
        assert_eq!(rigcfg_body(&saturated_rigcfg(), &mut out), RIGCFG_WORST);
        assert_eq!(
            riggen_body(&[b'z'; RIGGEN_MAX_GEN_BYTES], &mut out),
            RIGGEN_WORST
        );
        assert_eq!(
            capstat_body(&saturated_capture_status(), &mut out),
            CAPSTAT_WORST
        );
        assert_eq!(
            capmax_body(&saturated_ring_capacity(), &mut out),
            CAPMAX_WORST
        );
        assert_eq!(
            dumpend_body(&saturated_dump_summary(), &mut out),
            DUMPEND_WORST
        );
        assert_eq!(
            stimstart_body(&saturated_stimulus_start(), &mut out),
            STIMSTART_WORST
        );
    }
}
