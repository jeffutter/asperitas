//! Device-side console state: sequence numbers, cumulative loss counters, and the record
//! bodies whose bytes are a wire contract (`BOOT`, `STATUS`, and the six measurement-rig
//! verbs `RIGCFG`, `RIGGEN`, `CAPSTAT`, `CAPMAX`, `STIMSTART`, `DUMPEND`, plus the replay pair
//! `EXCPLAY` and `EXCPLAYFAIL`).
//!
//! # Why this module is not behind `log-usb`
//!
//! Everything here is plain arithmetic on `AtomicU32` and `core::fmt` — no critical
//! section, no clock, no pipe — so `cargo test --workspace` compiles and tests it on the
//! host even though the root workspace never enables `log-usb`, for the same reason
//! [`crate::frame`] is ungated. That matters because
//! [`status_body`] and [`StatusGate::due`] *are* the protocol: the protocol index documents the
//! field set and the rig runner parses it. A change to either should fail a host test in CI,
//! not first show up on a board three time zones away.
//!
//! The commit path that *uses* this lives in `lib.rs`; the drain task that reports it
//! lives in `usb.rs`.
//!
//! # Counter semantics
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
//!
//! # `BOOT` and `STATUS`: the reserved body prefixes
//!
//! Two bodies are wire contract and ride ordinary records (same framing, same CRC, same chance
//! of being dropped):
//!
//! ```text
//! BOOT proto=1 fw=0.1.0 pipe=2048 maxbody=200
//! STATUS proto=1 sent=12 dropped_full=3 bytes_dropped=4096 trunc=1 ep_err=2 seq_next=18 pipe_free=2048
//! ```
//!
//! `proto` comes first so a reader can handshake before trusting anything. `fw` is the
//! `asperitas-logging` crate version, which is the version that ships the console. `pipe` is the
//! log ring in bytes and `maxbody` the encoder's cap. `BOOT` consumes `seq 0` and is emitted only
//! once the USB backend is installed (`asperitas_logging::init` installs the logger, switches the
//! backend, then announces), so ordering it before the switch would discard it with no trace in
//! any counter.
//!
//! `STATUS` fields and their order are the contract; a unit test fails if either moves. Values
//! are absolute since-boot counts, never deltas.
//!
//! | Field | Counts |
//! |---|---|
//! | `sent` | Records committed to the ring whole. Cross-check against the host's decoded count. |
//! | `dropped_full` | Records refused for lack of space, never partially written. |
//! | `bytes_dropped` | Bytes those refused records would have occupied. |
//! | `trunc` | Bodies shortened by the 200-byte cap. Those records shipped, which is why they are not in `dropped_full`. |
//! | `ep_err` | Endpoint writes that failed: link loss or stalls. |
//! | `seq_next` | The value the next record will carry. |
//! | `pipe_free` | Ring free bytes at the instant of rendering. |
//!
//! Pacing: at most once per second ([`StatusGate`]) and only when a counter actually moved,
//! emitted from the drain loop only when the ring is empty. Debouncing is not politeness: during
//! a full-ring condition the `STATUS` record competes for the very space that is missing, so an
//! undebounced emitter starves the logs it is reporting on.
//!
//! CRC and `seq` detect damage, never absence. A gap is equally consistent with "bytes lost" and
//! "board rebooted and restarted numbering at 0"; the second `BOOT` (or a `seq` regression)
//! breaks the tie. There is deliberately no boot id in v1: it would need persistence across
//! reset for no gain over the banner's presence. A lost `STATUS` record is indistinguishable
//! from nothing having changed, except through the `seq` gap it leaves, so silence means
//! "probably nothing changed", not "nothing changed".
//!
//! # Rig record grammar
//!
//! Everything the measurement rig says that is not a log line. Bodies ride [`crate::frame`]
//! untouched, so `level`, `seq` and `t_ms` belong to the frame. Field names and order are the
//! contract, and each row is pinned by the host test named beside it. The worked lines are test
//! fixtures, not captures. The excerpt verbs are tabled in [`crate::excerpt`].
//!
//! | Record | Sent | Worked line | Pinned by |
//! |---|---|---|---|
//! | `RIGCFG` | once at boot | `RIGCFG proto=1 capture=mono16 lane=L blocks=879 block_bytes=32768 bytes_per_s=96000 capsec_us=349525333 window_s=300 cpu_hz=480000000 icache=1 dcache=0` | `rigcfg_body_pins_field_names_and_order` |
//! | `RIGGEN` | once at boot | `RIGGEN proto=1 name=sine sample_rate_hz=48000 level_dbfs=-20.0 frequency_hz=1000 period_samples=48` (generator text clipped to 160 B) | `riggen_body_carries_the_generator_text_verbatim` |
//! | `CAPMAX` | once at boot | `CAPMAX proto=1 total_bytes=28800000 ring_bytes=33554432 seconds_max=349 us_max=349525333 unused_headroom_bytes=4754432` | `capmax_body_pins_field_names_and_order` |
//! | `CAPSTAT` | about 1 Hz | `CAPSTAT proto=1 delivered=412 expected=467 overrun=0 max_block_us=731 worst_gap_us=1102 audio_exit=0 dumped=100 dropped_full=3` | `capstat_body_pins_field_names_and_order` |
//! | `STIMSTART` | once, one-shot stimulus builds | `STIMSTART proto=1 first_block=4 offset=0` | `stimstart_body_pins_field_names_and_order` |
//! | `AUDIO` | per chunk, during the dump | `AUDIO blk=1234 n=40 c=3f d=8J9hAH//XA==` (final chunk, 7 raw bytes) | `golden_padded_final_chunks_survive_framing` in `tests/console_dump.rs` |
//! | `AUDEND` | closes each block | `AUDEND blk=0000 n=03 bytes=00009 crc16=29b1` | `golden_audend_frame_is_reproduced_byte_for_byte` in `tests/console_dump.rs` |
//! | `DUMPEND` | once, the dump summary | `DUMPEND proto=1 blocks=879 chunks=224145 bytes=28800000 elapsed_ms=311402 refused=17 stall_ms=940 sent=4118 dropped_full=3 bytes_dropped=12288` | `dumpend_body_pins_field_names_and_order` |
//!
//! Unless a row names a file, the pinning test is in this module. `AUDIO` and `AUDEND` carry
//! their widths and size table in [`crate::dump`], which is normative for them. `RIGCFG` and
//! `CAPMAX` report constants the firmware was compiled with, not measurements. `CAPSTAT`
//! counters are since-boot. `DUMPEND` is the tightest body on the wire (194 of 200 bytes at
//! saturation). Its `elapsed_ms` is a reading; a dump rate computed from it is derived from that
//! reading (see [`crate::capture`]).

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::body::BodyWriter;
use crate::counter::SaturatingCounter;

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
    records_sent: SaturatingCounter,
    dropped_full: SaturatingCounter,
    bytes_dropped: SaturatingCounter,
    truncated: SaturatingCounter,
    endpoint_errors: SaturatingCounter,
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
            records_sent: SaturatingCounter::new(),
            dropped_full: SaturatingCounter::new(),
            bytes_dropped: SaturatingCounter::new(),
            truncated: SaturatingCounter::new(),
            endpoint_errors: SaturatingCounter::new(),
        }
    }

    /// Take the sequence number for the next record.
    ///
    /// Wraps mod 2^32 — see the module docs. Callers assign it **inside** the record
    /// lock, which is what makes numeric order equal wire order.
    #[deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]
    pub fn take_seq(&self) -> u32 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// A record reached the pipe whole.
    #[deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]
    pub fn record_committed(&self) {
        self.records_sent.bump();
    }

    /// A record was refused because it did not fit. `bytes` is the size of the frame that
    /// never went out, so the ring is untouched and only these two counters move.
    #[deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]
    pub fn record_dropped_for_space(&self, bytes: usize) {
        self.dropped_full.bump();
        self.bytes_dropped
            .add(u32::try_from(bytes).unwrap_or(u32::MAX));
    }

    /// A body exceeded [`crate::frame::MAX_BODY`] and shipped shortened.
    #[deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]
    pub fn body_shortened(&self) {
        self.truncated.bump();
    }

    /// An endpoint write failed.
    pub fn endpoint_error(&self) {
        self.endpoint_errors.bump();
    }

    /// Snapshot every counter in one place, so no caller hand-rolls six loads and gets
    /// the set out of date when a counter is added.
    pub fn snapshot(&self) -> ConsoleCounters {
        ConsoleCounters {
            records_sent: self.records_sent.load(),
            dropped_full: self.dropped_full.load(),
            bytes_dropped: self.bytes_dropped.load(),
            truncated: self.truncated.load(),
            endpoint_errors: self.endpoint_errors.load(),
            seq_next: self.seq.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// Record bodies that are wire format
// ---------------------------------------------------------------------------

/// Render the `BOOT` banner into `out`, returning the bytes written.
///
/// Field order is normative: `proto` first so a reader can handshake
/// before trusting anything, then what produced the capture. `fw_version` is the
/// `asperitas-wire` crate version, which is the version that ships the console — the
/// firmware binaries have no separate release process.
pub fn boot_body(
    out: &mut [u8; BODY_WINDOW],
    fw_version: &str,
    pipe_bytes: usize,
    max_body: usize,
) -> usize {
    let mut w = BodyWriter::new(out);
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
/// The field set and their order are the contract the protocol index documents and the rig runner
/// parses; the unit test below fails if either changes. Values are absolute
/// since-boot counts, never per-interval deltas — the host owns the differencing.
///
/// `pipe_free` is the ring's free bytes at the instant of rendering, which is what makes
/// a drop storm diagnosable after the fact: it shows how close to full the ring was
/// rather than merely that something was dropped.
pub fn status_body(out: &mut [u8; BODY_WINDOW], snap: &ConsoleCounters, pipe_free: usize) -> usize {
    let mut w = BodyWriter::new(out);
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

/// Whether `body` is a `BOOT` banner, the first record of a boot (see [`boot_body`]).
pub fn is_boot(body: &[u8]) -> bool {
    body.starts_with(b"BOOT ")
}

/// Sequence continuity across the validated records of one capture.
///
/// Consecutive numbering wraps at `u32::MAX` and the wrap is not a gap. A `BOOT` banner
/// restarts the device's numbering, so it counts as a restart whatever its `seq`, never as a
/// gap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Continuity {
    /// Records observed.
    pub observed: u64,
    /// Records whose `seq` did not follow the previous record's, excluding restarts.
    pub gaps: u64,
    /// `BOOT` banners that followed another record.
    pub restarts: u64,
    /// `(previous, next)` of the first gap.
    pub first_gap: Option<(u32, u32)>,
    previous: Option<u32>,
}

impl Continuity {
    /// Note one record.
    pub fn observe(&mut self, record: &crate::frame::Record<'_>) {
        self.observed += 1;
        if let Some(previous) = self.previous {
            if is_boot(record.body) {
                self.restarts += 1;
            } else if record.seq != previous.wrapping_add(1) {
                self.gaps += 1;
                self.first_gap.get_or_insert((previous, record.seq));
            }
        }
        self.previous = Some(record.seq);
    }
}

/// A parsed `STATUS` record body: the counterpart of [`status_body`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusRecord {
    /// Records the device has committed since boot.
    pub sent: u32,
    /// Records dropped because the ring was full.
    pub dropped_full: u32,
    /// Bytes those drops cost.
    pub bytes_dropped: u32,
    /// Records whose body was truncated.
    pub truncated: u32,
    /// USB endpoint write errors.
    pub endpoint_errors: u32,
    /// The `seq` the next record will carry.
    pub seq_next: u32,
    /// Free bytes in the ring when the record was rendered.
    pub pipe_free: u64,
}

impl StatusRecord {
    /// Parse a body rendered by [`status_body`]: protocol 1, all seven fields, in order.
    ///
    /// `None` for anything else - another protocol version, a missing, reordered or
    /// non-numeric field, or trailing text - so a host never reports a half-understood record
    /// as device truth.
    pub fn parse(body: &[u8]) -> Option<StatusRecord> {
        let text = core::str::from_utf8(body).ok()?;
        let mut tokens = text.split_ascii_whitespace();
        if tokens.next()? != "STATUS" {
            return None;
        }
        let mut field = |name: &str| -> Option<&str> {
            let (key, value) = tokens.next()?.split_once('=')?;
            (key == name).then_some(value)
        };
        if field("proto")? != "1" {
            return None;
        }
        let parsed = StatusRecord {
            sent: field("sent")?.parse().ok()?,
            dropped_full: field("dropped_full")?.parse().ok()?,
            bytes_dropped: field("bytes_dropped")?.parse().ok()?,
            truncated: field("trunc")?.parse().ok()?,
            endpoint_errors: field("ep_err")?.parse().ok()?,
            seq_next: field("seq_next")?.parse().ok()?,
            pipe_free: field("pipe_free")?.parse().ok()?,
        };
        tokens.next().is_none().then_some(parsed)
    }

    /// Read one counter from a `STATUS`-shaped body without requiring the rest of the format.
    ///
    /// [`parse`](Self::parse) is the strict contract; this is the host's tolerance for drift.
    /// It needs only the leading `STATUS` token and a `name=<u32>` token somewhere after it, so a
    /// reordered, extended, truncated or newer-protocol body still yields the counters a verdict
    /// depends on. `None` when the body is not `STATUS` or the counter is absent or non-numeric.
    pub fn counter(body: &[u8], name: &str) -> Option<u32> {
        let text = core::str::from_utf8(body).ok()?;
        let mut tokens = text.split_ascii_whitespace();
        if tokens.next()? != "STATUS" {
            return None;
        }
        tokens.find_map(|token| {
            let (key, value) = token.split_once('=')?;
            (key == name).then(|| value.parse().ok()).flatten()
        })
    }
}

// ---------------------------------------------------------------------------
// Measurement-rig verbs
// ---------------------------------------------------------------------------

/// How many decimal digits `value` renders as, with no leading zeros.
///
/// A `const fn` so a field kind can derive its width from the value type's maximum
/// (`decimal_digits(u32::MAX as u64)`) instead of restating the number: a wider type changes the
/// width and every verb's worst case follows it into the `MAX_BODY` asserts.
const fn decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

/// The longest string in `all`, in bytes.
const fn longest(all: &[&str]) -> usize {
    let mut i = 0;
    let mut max = 0;
    while i < all.len() {
        if all[i].len() > max {
            max = all[i].len();
        }
        i += 1;
    }
    max
}

/// One field kind a numeric verb body can carry: how it renders and the most bytes that takes.
///
/// The width is the single source for a verb's worst-case length, so a verb's field list is
/// written once, in [`wire_verb`], and drives both the rendering and the size check.
trait WireField {
    type Value;
    /// Bytes this field renders to at its worst.
    const WIDTH: usize;
    fn write(w: &mut BodyWriter<'_>, value: Self::Value);
}

/// A `u32` in plain decimal; ten digits at `u32::MAX`.
struct Dec;
impl WireField for Dec {
    type Value = u32;
    const WIDTH: usize = decimal_digits(u32::MAX as u64);
    fn write(w: &mut BodyWriter<'_>, value: u32) {
        let _ = core::write!(w, "{value}");
    }
}

/// A slot index in plain decimal; three digits at `u8::MAX`.
struct Slot;
impl WireField for Slot {
    type Value = u8;
    const WIDTH: usize = decimal_digits(u8::MAX as u64);
    fn write(w: &mut BodyWriter<'_>, value: u8) {
        let _ = core::write!(w, "{value}");
    }
}

/// A `u16` as four lowercase hex digits.
struct Hex4;
impl WireField for Hex4 {
    type Value = u16;
    const WIDTH: usize = (u16::BITS / 4) as usize;
    fn write(w: &mut BodyWriter<'_>, value: u16) {
        w.hex(u32::from(value), 4);
    }
}

/// A boolean as `0` or `1`.
struct Flag;
impl WireField for Flag {
    type Value = bool;
    const WIDTH: usize = 1;
    fn write(w: &mut BodyWriter<'_>, value: bool) {
        w.put(if value { b"1" } else { b"0" });
    }
}

/// The captured channel, as its one-letter spelling.
struct Lane;
impl WireField for Lane {
    type Value = MonoLane;
    const WIDTH: usize = longest(&MonoLane::SPELLINGS);
    fn write(w: &mut BodyWriter<'_>, value: MonoLane) {
        w.put(value.letter().as_bytes());
    }
}

/// A refusal reason, as wide as the longest spelling in [`PlayRefusal::SPELLINGS`].
struct Why;
impl WireField for Why {
    type Value = PlayRefusal;
    const WIDTH: usize = longest(&PlayRefusal::SPELLINGS);
    fn write(w: &mut BodyWriter<'_>, value: PlayRefusal) {
        w.put(value.as_str().as_bytes());
    }
}

/// Define a `name=value` verb body builder together with its worst-case length.
///
/// The field list is the only place a verb's fields are spelled: the same list renders the body
/// and, through each kind's [`WireField::WIDTH`], computes the `const` worst-case length that the
/// asserts below hold against [`crate::frame::MAX_BODY`]. A field added to the list therefore
/// fails the build if the verb stops fitting one record, rather than shipping a body that
/// silently truncates.
macro_rules! wire_verb {
    (
        $(#[$meta:meta])*
        pub fn $fn:ident($($arg:ident: $ty:ty),*) -> $worst:ident = $prefix:literal {
            $($name:ident: $kind:ty = $value:expr,)*
        }
    ) => {
        /// Worst-case body length of this verb: its literal prefix plus
        /// `1 + name + 1 + width` for every field.
        const $worst: usize = $prefix.len()
            $(+ 1 + stringify!($name).len() + 1 + <$kind as WireField>::WIDTH)*;

        $(#[$meta])*
        pub fn $fn($($arg: $ty,)* out: &mut [u8; BODY_WINDOW]) -> usize {
            let mut w = BodyWriter::new(out);
            w.put($prefix.as_bytes());
            $(
                w.put(concat!(" ", stringify!($name), "=").as_bytes());
                <$kind as WireField>::write(&mut w, $value);
            )*
            w.filled()
        }
    };
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
    /// Every wire spelling, kept beside [`Self::letter`]; a test fails if a variant is missing.
    const SPELLINGS: [&'static str; 2] = [MonoLane::Left.letter(), MonoLane::Right.letter()];

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
/// (both values fit, and the rig binary narrows explicitly) and is *not* a claim that a ring can never
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

wire_verb! {
    /// Render a `RIGCFG` record body into `out`, returning the bytes written.
    ///
    /// Carries numbers only. The generator's free-text description lives in its own
    /// [`riggen_body`] record, because a body mixing one unbounded string with nine numbers has no
    /// checkable bound: `RIGCFG`'s numeric fields leave 23 bytes of slack, less than the default
    /// pulse-train description (97), so any string that could actually be sent would blow the
    /// record. Splitting them keeps both verbs provably one-record.
    pub fn rigcfg_body(cfg: &RigConfig) -> RIGCFG_WORST = "RIGCFG proto=1 capture=mono16" {
        lane: Lane = cfg.lane,
        blocks: Dec = cfg.blocks,
        block_bytes: Dec = cfg.block_bytes,
        bytes_per_s: Dec = cfg.bytes_per_s,
        capsec_us: Dec = cfg.capsec_us,
        window_s: Dec = cfg.window_s,
        cpu_hz: Dec = cfg.cpu_hz,
        icache: Flag = cfg.icache,
        dcache: Flag = cfg.dcache,
    }
}

/// Most generator-description bytes [`riggen_body`] will carry: 160.
///
/// Named so the caller can check its own payload against the budget instead of trusting a
/// comment. `asperitas-stimulus` guarantees only that `describe()` stays under `frame::MAX_BODY`,
/// which is 16 bytes more than this verb can carry; `rig/main.rs` debug-asserts the length
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
/// Unlike its siblings this one does not go through `BodyWriter`. Its payload is `&[u8]`,
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

/// The byte budget a `CAPSTAT` record may never exceed: [`crate::frame::MAX_BODY`].
///
/// An alias rather than a second `200`, so the rate gate in `rig/geometry.rs` and the encoder cannot
/// drift apart while both claim to know the cap. It lives next to the builder that produces
/// the record and is checked by `capstat_body_saturated_counters_fit_one_frame`, because a
/// gate that divided by a hand-typed number would be arithmetic nobody tested.
pub const CAPSTAT_MAX_BODY: usize = crate::frame::MAX_BODY;

// The alias must never exceed what the encoder caps at. Checked at compile time rather than
// in the test below, because a comparison between two constants has one possible outcome and a
// runtime assertion on it can only ever pass; `clippy::assertions_on_constants` says so too.
const _: () = assert!(CAPSTAT_MAX_BODY <= crate::frame::MAX_BODY);

wire_verb! {
    /// Render a `CAPSTAT` record body into `out`, returning the bytes written.
    pub fn capstat_body(st: &CaptureStatus) -> CAPSTAT_WORST = "CAPSTAT proto=1" {
        delivered: Dec = st.delivered,
        expected: Dec = st.expected,
        overrun: Dec = st.overrun,
        max_block_us: Dec = st.max_block_us,
        worst_gap_us: Dec = st.worst_gap_us,
        audio_exit: Dec = st.audio_exit,
        dumped: Dec = st.dumped,
        dropped_full: Dec = st.dropped_full,
    }
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

wire_verb! {
    /// Render a `CAPMAX` record body into `out`, returning the bytes written.
    pub fn capmax_body(cap: &RingCapacity) -> CAPMAX_WORST = "CAPMAX proto=1" {
        total_bytes: Dec = cap.total_bytes,
        ring_bytes: Dec = cap.ring_bytes,
        seconds_max: Dec = cap.seconds_max,
        us_max: Dec = cap.us_max,
        unused_headroom_bytes: Dec = cap.unused_headroom_bytes,
    }
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

wire_verb! {
    /// Render a `STIMSTART` record body into `out`, returning the bytes written.
    pub fn stimstart_body(st: &StimulusStart) -> STIMSTART_WORST = "STIMSTART proto=1" {
        first_block: Dec = st.first_block,
        offset: Dec = st.offset,
    }
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

wire_verb! {
    /// Render a `DUMPEND` record body into `out`, returning the bytes written.
    pub fn dumpend_body(d: &DumpSummary) -> DUMPEND_WORST = "DUMPEND proto=1" {
        blocks: Dec = d.blocks,
        chunks: Dec = d.chunks,
        bytes: Dec = d.bytes,
        elapsed_ms: Dec = d.elapsed_ms,
        refused: Dec = d.refused,
        stall_ms: Dec = d.stall_ms,
        sent: Dec = d.sent,
        dropped_full: Dec = d.dropped_full,
        bytes_dropped: Dec = d.bytes_dropped,
    }
}

/// One completed replay pass, as one `EXCPLAY` record.
///
/// Sent once per boot, from thread mode, after the callback has handed the last source sample to the
/// DAC encoder.
///
/// # What `crc16` covers
///
/// The CRC-16/CCITT-FALSE of the samples the audio callback handed to the DAC encoder, in order
/// (`asperitas_replay`). `want` is the CRC the slot header stored when the excerpt was installed, which
/// the install verified against the host's. Equal CRCs with `clean=1` say the QSPI-to-DAC-encoder
/// path delivered the excerpt intact and unbroken. They say **nothing** about the codec, the analog
/// stage or the cable: what comes back through them is the capture's business.
#[derive(Clone, Copy, Debug)]
pub struct ExcerptPlay {
    /// Slot played, `0..excerpt::SLOT_COUNT`.
    pub slot: u8,
    /// Source bytes handed to the encoder: the excerpt's length once the pass is complete.
    pub bytes: u32,
    /// CRC-16 of those bytes as the callback handed them out.
    pub crc16: u16,
    /// The CRC the slot header stored.
    pub want: u16,
    /// Blocks of silence inserted because a staging half was late.
    pub underruns: u32,
    /// Whether the pass played without an underrun.
    pub clean: bool,
}

wire_verb! {
    /// Render an `EXCPLAY` record body into `out`, returning the bytes written.
    pub fn exc_play_body(play: &ExcerptPlay) -> EXCPLAY_WORST = "EXCPLAY proto=1" {
        slot: Slot = play.slot,
        bytes: Dec = play.bytes,
        crc16: Hex4 = play.crc16,
        want: Hex4 = play.want,
        underruns: Dec = play.underruns,
        clean: Flag = play.clean,
    }
}

/// Why a boot refused to play the slot it was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayRefusal {
    /// The slot holds no header at all: blank flash, or an install that never finished.
    Empty,
    /// The slot holds something that is not a valid header (wrong version, torn write, bad field).
    Invalid,
}

impl PlayRefusal {
    /// Every wire spelling, kept beside [`Self::as_str`]; a test fails if a variant is missing.
    const SPELLINGS: [&'static str; 2] =
        [PlayRefusal::Empty.as_str(), PlayRefusal::Invalid.as_str()];

    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PlayRefusal::Empty => "empty",
            PlayRefusal::Invalid => "invalid",
        }
    }
}

wire_verb! {
    /// Render an `EXCPLAYFAIL` record body into `out`, returning the bytes written.
    ///
    /// The record a boot sends instead of playing silence when its slot holds no playable excerpt.
    pub fn exc_play_refused_body(slot: u8, why: PlayRefusal) -> EXCPLAYFAIL_WORST = "EXCPLAYFAIL proto=1" {
        slot: Slot = slot,
        why: Why = why,
    }
}

// One record each, at their own worst case. These lines are the half a drifted format
// string breaks and a runtime test would only catch on the night it runs.
const _: () = assert!(EXCPLAY_WORST < crate::frame::MAX_BODY);
const _: () = assert!(EXCPLAYFAIL_WORST < crate::frame::MAX_BODY);
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
    fn counter_reads_through_format_drift() {
        let reordered = b"STATUS ep_err=4 dropped_full=7 proto=1";
        let newer = b"STATUS proto=2 sent=1 dropped_full=9 new=1";
        let truncated = b"STATUS proto=1 sent=5 dropped_full=3 byt";
        assert_eq!(StatusRecord::counter(reordered, "dropped_full"), Some(7));
        assert_eq!(StatusRecord::counter(reordered, "ep_err"), Some(4));
        assert_eq!(StatusRecord::counter(newer, "dropped_full"), Some(9));
        assert_eq!(StatusRecord::counter(truncated, "dropped_full"), Some(3));
        assert_eq!(StatusRecord::counter(truncated, "ep_err"), None);
        assert_eq!(
            StatusRecord::counter(b"BOOT dropped_full=1", "dropped_full"),
            None
        );
        assert_eq!(
            StatusRecord::counter(b"STATUS dropped_full=x", "dropped_full"),
            None
        );
        assert_eq!(StatusRecord::counter(b"\xff", "dropped_full"), None);
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

    fn observed(c: &mut Continuity, seq: u32, body: &[u8]) {
        c.observe(&crate::frame::Record {
            level: b'I',
            seq,
            t_ms: 0,
            body,
        });
    }

    #[test]
    fn continuity_counts_gaps_and_restarts_apart_and_ignores_the_seq_wrap() {
        let mut c = Continuity::default();
        observed(&mut c, u32::MAX - 1, b"x");
        observed(&mut c, u32::MAX, b"x");
        observed(&mut c, 0, b"x");
        assert_eq!((c.gaps, c.restarts, c.first_gap), (0, 0, None));
        observed(&mut c, 5, b"x");
        observed(&mut c, 9, b"x");
        observed(&mut c, 0, b"BOOT proto=1");
        assert_eq!(
            (c.observed, c.gaps, c.restarts, c.first_gap),
            (6, 2, 1, Some((0, 5)))
        );
    }

    #[test]
    fn status_record_parses_what_status_body_renders() {
        let mut out = [0u8; BODY_WINDOW];
        for (snap, free) in [
            (counters(12, 3, 4_096, 1, 2, 18), 2_048),
            (
                counters(u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX),
                usize::MAX,
            ),
        ] {
            let n = status_body(&mut out, &snap, free);
            let parsed = StatusRecord::parse(&out[..n]).expect("own rendering parses");
            assert_eq!(
                parsed,
                StatusRecord {
                    sent: snap.records_sent,
                    dropped_full: snap.dropped_full,
                    bytes_dropped: snap.bytes_dropped,
                    truncated: snap.truncated,
                    endpoint_errors: snap.endpoint_errors,
                    seq_next: snap.seq_next,
                    pipe_free: free as u64,
                }
            );
        }
    }

    #[test]
    fn status_record_refuses_what_it_does_not_fully_understand() {
        let good = "STATUS proto=1 sent=5 dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100";
        assert!(StatusRecord::parse(good.as_bytes()).is_some());
        for bad in [
            "STATUS proto=2 sent=5 dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100",
            "STATUS proto=2 dropped_full=9",
            "STATUS proto=1 dropped_full=0",
            "STATUS proto=1 sent=x dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100",
            "BOOT proto=1 sent=5 dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100",
            "STATUS proto=1 sent=5 dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100 extra=1",
        ] {
            assert_eq!(StatusRecord::parse(bad.as_bytes()), None, "{bad}");
        }
        assert_eq!(StatusRecord::parse(&[0xff, 0xfe]), None);
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
        // way to land on the ceiling: `bump` and `add` share one saturating path.
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

    /// `asperitas-stimulus`'s pinned description of a 1 kHz sine (`stimulus_tests.rs`), used here as
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
    fn excplay_body_pins_field_names_and_order() {
        let mut out = [0u8; BODY_WINDOW];
        let play = ExcerptPlay {
            slot: 3,
            bytes: 288_000,
            crc16: 0x0abc,
            want: 0x0abc,
            underruns: 0,
            clean: true,
        };
        let n = exc_play_body(&play, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "EXCPLAY proto=1 slot=3 bytes=288000 crc16=0abc want=0abc underruns=0 clean=1"
        );
    }

    #[test]
    fn excplay_body_renders_worst_case_inside_its_table() {
        let mut out = [0u8; BODY_WINDOW];
        let play = ExcerptPlay {
            slot: 255,
            bytes: u32::MAX,
            crc16: 0xffff,
            want: 0x0000,
            underruns: u32::MAX,
            clean: false,
        };
        let n = exc_play_body(&play, &mut out);
        assert_eq!(n, EXCPLAY_WORST);
        assert!(n < crate::frame::MAX_BODY);
        assert!(core::str::from_utf8(&out[..n])
            .unwrap()
            .ends_with("clean=0"));
    }

    #[test]
    fn excplayfail_body_pins_field_names_and_fits() {
        let mut out = [0u8; BODY_WINDOW];
        let n = exc_play_refused_body(13, PlayRefusal::Empty, &mut out);
        assert_eq!(
            core::str::from_utf8(&out[..n]).unwrap(),
            "EXCPLAYFAIL proto=1 slot=13 why=empty"
        );
        let n = exc_play_refused_body(255, PlayRefusal::Invalid, &mut out);
        assert_eq!(n, EXCPLAYFAIL_WORST);
    }

    /// The spelling tables behind `Lane` and `Why` must name every variant: the exhaustive
    /// matches fail to compile when a variant is added, and the length checks then fail until it
    /// reaches the table the width is derived from.
    #[test]
    fn spelling_tables_cover_every_variant() {
        for lane in [MonoLane::Left, MonoLane::Right] {
            match lane {
                MonoLane::Left | MonoLane::Right => {}
            }
            assert!(MonoLane::SPELLINGS.contains(&lane.letter()));
        }
        for why in [PlayRefusal::Empty, PlayRefusal::Invalid] {
            match why {
                PlayRefusal::Empty | PlayRefusal::Invalid => {}
            }
            assert!(PlayRefusal::SPELLINGS.contains(&why.as_str()));
        }
    }

    /// Render `value` through field kind `K`, returning the bytes written.
    fn rendered_len<K: WireField>(value: K::Value) -> usize {
        let mut buf = [0u8; 32];
        let mut w = BodyWriter::new(&mut buf);
        K::write(&mut w, value);
        w.filled()
    }

    #[test]
    fn each_field_kind_renders_exactly_its_declared_width_at_saturation() {
        assert_eq!(rendered_len::<Dec>(u32::MAX), Dec::WIDTH);
        assert_eq!(Dec::WIDTH, 10);
        assert_eq!(rendered_len::<Slot>(u8::MAX), Slot::WIDTH);
        assert_eq!(Slot::WIDTH, 3);
        assert_eq!(rendered_len::<Hex4>(u16::MAX), Hex4::WIDTH);
        assert_eq!(rendered_len::<Hex4>(0), Hex4::WIDTH, "hex is zero padded");
        assert_eq!(rendered_len::<Flag>(true), Flag::WIDTH);
        assert_eq!(rendered_len::<Flag>(false), Flag::WIDTH);
        for lane in [MonoLane::Left, MonoLane::Right] {
            assert!(rendered_len::<Lane>(lane) <= Lane::WIDTH);
        }
        let widest = [PlayRefusal::Empty, PlayRefusal::Invalid]
            .map(rendered_len::<Why>)
            .into_iter()
            .max();
        assert_eq!(widest, Some(Why::WIDTH));
        assert_eq!(Why::WIDTH, 7);
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

    /// Each builder's actual worst case must equal the width its field list claims. The `const`
    /// asserts above prove the list fits one record; these prove the rendered bytes still agree
    /// with the width each field kind declares.
    #[test]
    fn saturated_renders_match_their_declared_widths() {
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
        assert_eq!(
            exc_play_refused_body(u8::MAX, PlayRefusal::Invalid, &mut out),
            EXCPLAYFAIL_WORST
        );
    }
}
