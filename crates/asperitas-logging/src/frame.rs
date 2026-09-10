//! Console protocol v1 — every log record carries its own framing and checksum.
//!
//! # Why this module is not behind `log-usb`
//!
//! The codec is pure byte arithmetic, so CI must be able to exercise it on the host:
//! `cargo test --workspace` builds this crate with default features, i.e. without any
//! backend at all.
//!
//! # Why framing exists
//!
//! CDC-ACM hands the host an undifferentiated byte stream: no message boundaries,
//! no length, no sequence. A record whose tail vanished at a ring-buffer wrap is
//! therefore *indistinguishable from a short record*, which is how truncated knob
//! lines came to be filed as an ADC glitch (TASK-018.04). v1 makes damage visible
//! while keeping every record human-legible: `screen` output stays readable, `grep`
//! on a raw capture keeps working, and a reader can tell validated data from loss.
//!
//! Binary framing (COBS/SLIP) was considered and rejected for this project-specific
//! reason: it buys instant resynchronisation at the price of a capture nobody can
//! read. Substituting dangerous bytes at the producer is also the recognised fix for
//! log injection (CWE-117). The closest published analogue is NMEA-0183 — printable
//! sentence, start character, `*` + checksum over everything between the delimiters,
//! mandatory CRLF, and a hard maximum sentence length that is part of the grammar
//! rather than a convention.
//!
//! # Grammar (normative: TASK-030 §3)
//!
//! ```text
//! record := '~' level SP seq SP t_ms SP body '*' crc CRLF
//!
//! ~I 00000042 00004567 ENC +1*9c17\r\n
//! ^  ^ ^       ^       ^   ^    ^
//! │  │ │       │       │   │    └── CRLF: delimiter, guaranteed unique by sanitisation
//! │  │ │       │       │   └────── 4 lowercase hex digits, CRC-16/CCITT-FALSE
//! │  │ │       │       └────────── '*' trailer marker
//! │  │ │       └────────────────── body, 0..=200 bytes, no CR/LF/DEL
//! │  │ └────────────────────────── t_ms: 8 decimal digits (ms since boot, wraps ~27.8 h)
//! │  └──────────────────────────── seq: 8 lowercase hex digits, monotonic within a boot
//! │  └──────────────────────────── level: I W E D T
//! └─────────────────────────────── start marker, device→host only ('>' is reserved for host→device)
//! ```
//!
//! The CRC covers `level SP seq SP t_ms SP body` — everything after `~` up to but not
//! including `*`. Fixed-width fields mean spaces inside a body cannot confuse field
//! splitting, and sanitisation means the first CRLF after a start marker *must* be
//! that record's terminator.
//!
//! # What a CRC can and cannot prove
//!
//! Integrity, never absence. Two limits are load-bearing for anyone reading a
//! capture, so they belong here rather than only in a test:
//!
//! - A record whose leading `~` was lost in transit yields **no** integrity failure:
//!   the decoder never saw a candidate start. Its bytes surface as discarded bytes,
//!   and only a `seq` gap or a `STATUS` counter reveals the loss.
//! - A byte-level splice between two producers legitimately decodes as two good
//!   records plus one integrity failure, because the frames around the splice really
//!   are intact. The guarantee is that no record is ever *invented*, not that nothing
//!   decodes.
//! - Integrity is not authenticity. The checksum is affine over GF(2), so two edits
//!   whose contributions cancel leave it correct while changing the payload — see
//!   `crc_can_be_forged_at_weight_two` in `tests/console_frame.rs`, which constructs
//!   such a frame and watches it validate cleanly. Counters describe what the wire did
//!   to bytes, not who wrote them.
//!
//! Continuity is the decoder's and the reader's business (`seq`, plus the `BOOT`
//! record that distinguishes a restart from loss); this module's business is that a
//! record which validates is byte-identical to one that was written.
//!
//! See [`Decoder`] for the consumer half.
//!
//! # Offset derivation
//!
//! Every offset in this module comes from one quantity: `j`, the absolute index of
//! CR. Getting these inconsistent is how a parser silently rejects every max-size
//! record, so they are tabulated once.
//!
//! | quantity | expression | empty body | body `ENC +1` | body 200 B |
//! |---|---|---|---|---|
//! | total frame length | `j - start + 2` | 28 | 34 | 228 |
//! | `j - start` | `MIN_CR_OFFSET + body_len` | 26 | 32 | 226 |
//! | `'*'` | `j - 5` | 21 | 27 | 221 |
//! | CRC digits | `j - 4 .. j` | | | |
//! | body | `start + PREFIX_LEN .. j - 5` | ∅ | `ENC +1` | 200 × `a` |
//! | CRC-covered range | `start + 1 .. j - 5` | 20 B | 26 B | 220 B |
//!
//! `'*'` is therefore located **from `j`**, never by searching forward from the start:
//! sanitisation only neutralises control bytes and DEL, so a body may legitimately
//! contain `'*'`, and a forward search mis-decodes exactly those records.

use log::Level;

// ---------------------------------------------------------------------------
// Geometry — every offset in this module derives from these five numbers
// ---------------------------------------------------------------------------

/// Longest body that ships. Longer input is capped **before** the CRC is computed,
/// so a truncated record still validates its own checksum.
pub const MAX_BODY: usize = 200;

/// `'~' level SP seq(8) SP t_ms(8) SP` — fixed width, so field offsets are constants.
pub const PREFIX_LEN: usize = 21;

/// `'*' crc(4) CR LF`.
pub const TRAILER_LEN: usize = 7;

/// Longest possible record: 21 + 200 + 7. Overhead is 28 bytes per record.
pub const MAX_FRAME: usize = PREFIX_LEN + MAX_BODY + TRAILER_LEN;

/// Index of CR relative to the start marker for the shortest legal record (empty
/// body): `PREFIX_LEN + TRAILER_LEN - 2`, i.e. CR sits 2 bytes before the end.
pub const MIN_CR_OFFSET: usize = PREFIX_LEN + TRAILER_LEN - 2;

const _: () = assert!(MAX_FRAME == 228);
const _: () = assert!(MIN_CR_OFFSET + MAX_BODY + 2 == MAX_FRAME);

/// `t_ms` is transmitted modulo this so the prefix stays 8 digits wide forever.
/// Raw milliseconds exceed 8 digits after ~100 000 s (~27.8 h); without the modulo
/// the prefix would silently widen and invalidate every offset above. Neither field
/// reveals a wrap on its own — continuity comes from `BOOT` plus `seq`.
const T_MS_WRAP: u32 = 100_000_000;

// ---------------------------------------------------------------------------
// CRC-16/CCITT-FALSE
// ---------------------------------------------------------------------------

/// CRC-16/CCITT-FALSE over `data`.
///
/// Parameters, pinned explicitly because the name alone is a known trap:
///
/// | | |
/// |---|---|
/// | width | 16 |
/// | poly | `0x1021` |
/// | init | `0xFFFF` |
/// | refin / refout | `false` / `false` |
/// | xorout | `0x0000` |
/// | check | `0x29b1` (the 9-byte ASCII string `123456789`) |
/// | alias | CRC-16/IBM-3740 |
///
/// "CRC-16/CCITT" is commonly misidentified: the true CCITT/V.41 form is reflected
/// (that is KERMIT, check `0x2189`) and XMODEM is the init-`0x0000` variant. Anyone
/// renaming or "fixing" this function needs the parameters above, not the label.
/// Catalogue: <https://reveng.sourceforge.io/crc-catalogue/16.htm>
///
/// Deliberately a table-less bit loop rather than a 256-entry table: identical
/// source on both ends of the link, no extra data segment, and the cost is at most
/// 220 × 16 = 3 520 iterations per record. Measuring what that costs inside the
/// device's locked region is TASK-030.04's job, not a comment's.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            // `<<` on u16 discards the carry, so mask the polynomial feedback in.
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// What [`encode`] produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// Length of the framed record at `out[..len]`; bytes past it are untouched.
    pub len: usize,
    /// The body exceeded [`MAX_BODY`] and shipped shortened. Callers count this as
    /// `trunc`, never as a dropped record and never as an unterminated record: the
    /// record arrived and validates.
    pub truncated: bool,
}

/// Frame one record into `out`, returning what landed there.
///
/// `(level, seq, now_ms, body)` all come from the caller and this function reads no
/// clock, no counter and no global: sequencing and timing belong to the device log
/// path (TASK-030.02), which keeps this codec deterministic and testable. `out` is
/// likewise the caller's — the device passes its one lock-guarded buffer, tests pass
/// stack arrays — so this module holds no static and needs no `unsafe`.
///
/// The body is sanitised and capped on the way in ([`sanitize_byte`]), then the CRC
/// is computed over the bytes that actually ship.
pub fn encode(
    level: Level,
    seq: u32,
    now_ms: u32,
    body: &[u8],
    out: &mut [u8; MAX_FRAME],
) -> Encoded {
    // Body first: the prefix is fixed-width, so nothing here needs the body length
    // before writing the fields that precede it.
    let body_len = body.len().min(MAX_BODY);
    for (slot, &byte) in out[PREFIX_LEN..PREFIX_LEN + body_len].iter_mut().zip(body) {
        *slot = sanitize_byte(byte);
    }

    out[0] = b'~';
    out[1] = level_letter(level);
    out[2] = b' ';
    write_hex(seq, &mut out[3..11]);
    out[11] = b' ';
    write_decimal(now_ms % T_MS_WRAP, &mut out[12..20]);
    out[20] = b' ';

    let crc = crc16_ccitt(&out[1..PREFIX_LEN + body_len]);
    let trailer = PREFIX_LEN + body_len;
    out[trailer] = b'*';
    write_hex(u32::from(crc), &mut out[trailer + 1..trailer + 5]);
    out[trailer + 5] = b'\r';
    out[trailer + 6] = b'\n';

    Encoded {
        len: trailer + TRAILER_LEN,
        truncated: body.len() > MAX_BODY,
    }
}

/// The single wire letter for a level.
fn level_letter(level: Level) -> u8 {
    match level {
        Level::Error => b'E',
        Level::Warn => b'W',
        Level::Info => b'I',
        Level::Debug => b'D',
        Level::Trace => b'T',
    }
}

/// Map one body byte to something that can never forge a delimiter.
///
/// Control bytes (`< 0x20`, which covers CR and LF) and DEL (`0x7F`) become `'_'`.
/// Bytes `>= 0x80` pass through untouched so UTF-8 messages survive intact.
///
/// Printable punctuation deliberately survives — including `~`, `*` and `|`. Framing
/// strength therefore comes from the grammar plus the CRC and, above all, the
/// no-CR/LF invariant, not from a supposedly tilde-free payload: a stray `~` or `*`
/// inside a body can only produce a CRC mismatch, never a false-valid record.
pub const fn sanitize_byte(byte: u8) -> u8 {
    if byte < 0x20 || byte == 0x7F {
        b'_'
    } else {
        byte
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Render `value` as exactly `dst.len()` lowercase hex digits, zero-padded, MS digit
/// first. Width comes from the slice so the call site shows the field it fills.
///
/// `pub(crate)` so `dump` writes its own fields through the same renderer: one owner of
/// digit formatting means the record bodies' fixed widths cannot drift apart. Digits
/// past `dst.len()` are dropped, which is why callers mask to their field's modulus
/// rather than relying on truncation (`dump`'s `blk` wrap is explicit about it).
pub(crate) fn write_hex(value: u32, dst: &mut [u8]) {
    debug_assert!(dst.len() <= 8, "hex field wider than a u32");
    let n = dst.len();
    for (i, slot) in dst.iter_mut().enumerate() {
        let shift = 4 * (n - 1 - i);
        *slot = HEX_DIGITS[((value >> shift) & 0xF) as usize];
    }
}

/// Render `value` as exactly `dst.len()` decimal digits, zero-padded. The caller is
/// responsible for fitting (see [`T_MS_WRAP`]; `dump` derives its own field width from
/// the largest value the grammar can produce).
pub(crate) fn write_decimal(value: u32, dst: &mut [u8]) {
    let fits = value < 10u32.pow(dst.len() as u32);
    debug_assert!(fits, "value {value} does not fit in {} digits", dst.len());
    let mut v = value;
    for slot in dst.iter_mut().rev() {
        *slot = b'0' + (v % 10) as u8;
        v /= 10;
    }
}

// ---------------------------------------------------------------------------
// Whole-record-or-nothing commit
// ---------------------------------------------------------------------------

/// What one [`write_whole`] call did to its sink. Ignoring this value is a bug: the
/// `Stalled` variant is how a broken sink reaches the fail-loud path.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// Every byte of the frame is in the sink, contiguously and in order.
    Committed,
    /// The frame did not fit the capacity pre-check. `write` was called zero times.
    RefusedForSpace,
    /// The sink stopped accepting bytes **after** the pre-check passed — a violation of
    /// the caller's contract. `write_whole` does not panic on it: see the doc section
    /// below for why the panic belongs to the caller, and where it may fire.
    Stalled,
}

/// Commit `frame` to a sink, or refuse it whole. Three outcomes, reported as a
/// [`WriteOutcome`]:
///
/// - `RefusedForSpace` — the frame did not fit in `free_capacity`, and `write` was
///   called **zero** times. Nothing reached the sink, so nothing needs unwinding.
/// - `Committed` — every byte of `frame` is in the sink, contiguously and in order.
/// - `Stalled` — the sink stopped accepting bytes mid-frame, breaking the caller's
///   contract. Some bytes may have landed.
///
/// Whole-record-or-nothing therefore does not promise that a failed call leaves no
/// trace in the sink — on `Stalled`, some bytes genuinely did land. What it promises is
/// that a truncated record is *detectable downstream*: framing and the block CRC exist
/// to expose a half-written record to the host, not to hide it. There is deliberately
/// no partial-write count in the API: the caller cannot act on one, and anything it
/// would do next runs in the same broken-contract state that produced it.
/// Reserve/commit-with-discard is the same vocabulary Linux's ring buffer, printk's
/// `prb_reserve`/`prb_commit` and bitdrift's reserve/commit buffer use.
///
/// # Why the loop is required, not defensive
///
/// `embassy_sync::pipe::Pipe::try_write` short-writes at **every ring wrap even when
/// the ring is empty**: `RingBuffer::push_buf` returns only the contiguous run to the
/// end of the backing array (`embassy-sync 0.6.2/src/ring_buffer.rs:19-31`) while
/// `free_capacity()` reports *total* free (`pipe.rs:456`). Measured on the host: empty
/// ring, `free_capacity() == 512`, a 200-byte write accepts 112. A "check capacity,
/// then write once" fix therefore truncates records exactly as badly as the code it
/// replaces — hence the loop. Two rounds always suffice, because after crossing the
/// wrap the contiguous run equals total free space and a consumer can only add more.
///
/// # A stall after the pre-check is reported here, panicked by the caller
///
/// Given the pre-check plus the caller holding its lock, a stall mid-loop
/// (`None`/`Some(0)` once some bytes have landed) means the sink broke the caller's
/// contract. `write_whole` reports that as `WriteOutcome::Stalled` and never panics
/// itself. The **caller** must panic on it — loudly, in every profile, because
/// `debug_assert!` would vanish from the release build that ships — but only *after*
/// whatever lock protected the call has been released. This target aborts on panic: a
/// panic raised inside a `critical_section::with` closure never restores `PRIMASK`, so
/// interrupting the board permanently, on top of silencing the panic text (the serial
/// emit needs the USB interrupt it just masked), is strictly worse than the crash it
/// replaces. In this crate `commit_records` (`lib.rs`) owns that rule for both
/// `emit()` and `try_emit_dump()`; any future caller must follow it.
///
/// Retrying instead of failing loud is not a milder option: nothing about the sink or the
/// remaining slice changes between one retry and the next, so a genuine stall is
/// unbounded, not transient. The loop would hold the caller's lock forever with no
/// diagnostic at all, which is strictly worse than the crash it avoids. Panicking hands
/// the failure to the project's fail-loud path: the shared `panic_handler` module (`boot-led`
/// feature) turns the LED red and emits the panic text — this message included — over
/// whatever transport is compiled in.
pub fn write_whole(
    frame: &[u8],
    free_capacity: usize,
    mut write: impl FnMut(&[u8]) -> Option<usize>,
) -> WriteOutcome {
    if frame.len() > free_capacity {
        return WriteOutcome::RefusedForSpace;
    }

    let mut written = 0usize;
    while written < frame.len() {
        match write(&frame[written..]) {
            Some(n) if n > 0 => written += n,
            // `None` or `Some(0)` after the pre-check passed: the sink broke the
            // caller's contract. Reported, not panicked — see the doc section above.
            _ => return WriteOutcome::Stalled,
        }
    }
    WriteOutcome::Committed
}

// ---------------------------------------------------------------------------
// Decoder — incremental, allocation-free, chunk-boundary independent
// ---------------------------------------------------------------------------

/// One validated record: its framing *and* its CRC both checked out.
///
/// The body borrows the decoder's delivery queue, so a [`Decoder`] hands out one record
/// at a time and that record stays valid only until the next
/// [`push`](Decoder::push) or [`next_record`](Decoder::next_record): use each record
/// before asking for the next. That is deliberate — it avoids a self-referential return
/// type and avoids making every caller supply a buffer, and both the decode example and
/// the tests want exactly this shape. Returning owned `Vec`s instead would put an
/// allocator on the table for no gain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    /// Wire letter: `I`, `W`, `E`, `D` or `T`.
    pub level: u8,
    /// Sequence number as transmitted (8 hex digits).
    pub seq: u32,
    /// Milliseconds since boot as transmitted — already modulo `1e8`, so a reader
    /// cannot distinguish a wrap from a stall on this field alone.
    pub t_ms: u32,
    /// Body exactly as it arrived on the wire, hence already sanitised: control bytes
    /// and DEL show up as `'_'`, and a body longer than [`MAX_BODY`] arrives capped.
    pub body: &'a [u8],
}

/// What a [`Decoder`] saw, in four counters that each answer one question.
///
/// Read these only after [`Decoder::finish`] — see there. The accounting law
///
/// ```text
/// bytes_pushed == Σ consumed + discarded_bytes + buffered()
/// ```
/// holds after every [`push`](Decoder::push) and after `finish()`, which is what makes
/// the summary trustworthy rather than merely plausible.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Records that validated on framing *and* CRC.
    pub records: u64,
    /// Candidate start markers examined and rejected.
    pub bad_frames: u64,
    /// Rejections after which a later start marker was actually located. Always
    /// `<= bad_frames`; the two differ only when corruption runs to the end of a
    /// capture and no further marker turns up.
    pub resyncs: u64,
    /// Bytes dropped that never reached a validated record: inter-record junk, the
    /// forfeited `~` of each rejected start, and windows abandoned because no start
    /// marker ever appeared in them.
    pub discarded_bytes: u64,
}

/// How many validated records may sit undelivered at once.
///
/// Delivery is a queue rather than a single slot because one chunk routinely validates
/// several records: 4 KiB, the size TASK-032 reads, can hold 145 of the shortest legal
/// ones. A single slot would leave [`Decoder::push`] choosing between dropping bytes it
/// was handed and overwriting a record nobody had read yet — both ways a reader loses
/// history without learning anything happened.
///
/// Overflow cannot lose anything either: with a full queue `push` stops taking input
/// and returns a short count, so the bytes stay where the caller put them. The queue
/// therefore sets how much a reader may batch, not how much it may lose; eight is ample
/// for anything that drains between pushes and costs 1.7 KB of inline state.
pub const RECORD_SLOTS: usize = 8;

/// Incremental decoder for console protocol v1.
///
/// Feed bytes as they arrive — [`push`](Decoder::push) accepts any chunking,
/// including one record split across ten calls — take validated records from
/// [`next_record`](Decoder::next_record), and read [`stats`](Decoder::stats) only
/// after calling [`finish`](Decoder::finish).
///
/// # Why the parse cannot wedge, and why chunking cannot change the answer
///
/// Two states only: scanning for a start marker, and deciding the candidate already
/// located. Every decision uses only bytes already offered, which is what makes both
/// the records *and* the statistics identical under any chunking — a formulation with
/// lookahead (“resynchronise at the next `~`”, which an earlier planning pass wrote)
/// reports different counters depending on where the chunk boundaries fell.
///
/// Termination comes from the grammar: a well-formed body contains no CR and no LF
/// ([`sanitize_byte`]) and neither does the fixed-width prefix, so the first complete
/// CRLF after a candidate start *must* be that record's terminator. A candidate that
/// fails against it can never succeed later, so it is disqualified permanently; a
/// window that reaches [`MAX_FRAME`] with no terminator likewise cannot hold a record,
/// because nothing valid is longer. Each rejected byte is dropped once and each
/// examination inspects at most [`MAX_FRAME`] bytes, so the cost per input byte is
/// bounded independently of history. That is not plain `O(n)` in the worst case — many
/// `~`s inside one corrupt span each get their own ≤ 228-byte examination — and no
/// claim stronger than “bounded per byte” is being made.
///
/// # Resynchronisation
///
/// On failure the decoder advances **strictly past** the disqualified start byte and
/// never guesses a shorter body. This refines TASK-030 §3's “discard one candidate
/// start and retry at the next `~`”: same contract, decided locally, therefore
/// chunk-independent. The hazard worth naming is documented upstream — ArduPilot's C
/// MAVLink parser desynchronised permanently when a bad-CRC message happened to end in
/// a byte equal to the STX magic, because resuming at “the next plausible-looking byte”
/// can land *inside* the next real frame
/// (<https://github.com/ArduPilot/pymavlink/issues/881>). Records are emitted only
/// from fully validated frames, so a splice can cost a record but cannot invent one.
///
/// # Memory
///
/// Inline storage sized by [`MAX_FRAME`]: no allocator, no `unsafe`, and never more
/// than [`MAX_FRAME`] undecided bytes held at once.
///
/// # Example
///
/// ```
/// use asperitas_logging::frame::{encode, Decoder, MAX_FRAME};
/// use asperitas_logging::Level;
///
/// let mut buf = [0u8; MAX_FRAME];
/// let len = encode(Level::Info, 0x42, 4567, b"ENC +1", &mut buf).len;
///
/// let mut decoder = Decoder::new();
/// decoder.push(&buf[..3]);
/// assert!(decoder.next_record().is_none(), "three bytes decide nothing");
/// decoder.push(&buf[3..len]);
///
/// let record = decoder.next_record().expect("framing and CRC both validate");
/// assert_eq!((record.level, record.seq, record.t_ms), (b'I', 0x42, 4567));
/// assert_eq!(record.body, b"ENC +1");
///
/// decoder.finish();
/// assert_eq!(decoder.buffered(), 0, "finish() abandons the pending window");
/// let stats = decoder.stats();
/// assert_eq!((stats.records, stats.bad_frames, stats.discarded_bytes), (1, 0, 0));
/// ```
pub struct Decoder {
    /// Sliding window; `[0..len]` is what has been offered and not yet decided.
    buf: [u8; MAX_FRAME],
    len: usize,
    /// `true` while deciding the candidate at `buf[0]`; `false` while scanning for a
    /// start marker.
    in_candidate: bool,
    /// Set on rejection, consumed when the next start marker is found — counting a
    /// resynchronisation when the marker is located rather than when the rejection
    /// happens is what keeps `resyncs` chunk-independent.
    armed_resync: bool,
    /// Validated records awaiting a reader, oldest at `head`.
    queue_levels: [u8; RECORD_SLOTS],
    queue_seqs: [u32; RECORD_SLOTS],
    queue_times: [u32; RECORD_SLOTS],
    queue_bodies: [[u8; MAX_BODY]; RECORD_SLOTS],
    queue_body_lens: [usize; RECORD_SLOTS],
    /// Queue bounds: `head <= tail <= head + RECORD_SLOTS`, both only ever increasing,
    /// so slot index is `count % RECORD_SLOTS` without a wrap flag.
    head: u64,
    tail: u64,
    stats: Stats,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// A decoder with no bytes offered and all counters zero.
    pub const fn new() -> Self {
        Self {
            buf: [0u8; MAX_FRAME],
            len: 0,
            in_candidate: false,
            armed_resync: false,
            queue_levels: [0u8; RECORD_SLOTS],
            queue_seqs: [0u32; RECORD_SLOTS],
            queue_times: [0u32; RECORD_SLOTS],
            queue_bodies: [[0u8; MAX_BODY]; RECORD_SLOTS],
            queue_body_lens: [0usize; RECORD_SLOTS],
            head: 0,
            tail: 0,
            stats: Stats {
                records: 0,
                bad_frames: 0,
                resyncs: 0,
                discarded_bytes: 0,
            },
        }
    }

    /// Take as many of `bytes` as the decoder can currently hold, returning how many it
    /// took.
    ///
    /// Any chunking is allowed, including a record split across ten calls. Records
    /// become available from [`next_record`](Decoder::next_record) as soon as framing
    /// and CRC confirm them; nothing waits for a boundary the wire does not provide.
    ///
    /// A count shorter than `bytes.len()` means the decoder is full — either its
    /// delivery queue is waiting to be drained or its window is holding a maximum-length
    /// candidate — and the untouched bytes remain the caller's to submit again. Stopping
    /// there is deliberate: taking bytes it cannot hold would mean losing records
    /// silently, which is the very failure this protocol exists to make impossible. So
    /// the lossless reader loop is drain, then retry:
    ///
    /// ```text
    /// let mut off = 0;
    /// while off < buf.len() {
    ///     off += decoder.push(&buf[off..]);
    ///     while let Some(record) = decoder.next_record() {
    ///         emit(record);
    ///     }
    /// }
    /// ```
    pub fn push(&mut self, bytes: &[u8]) -> usize {
        let mut pos = 0usize;
        while pos < bytes.len() && !self.delivery_full() {
            let take = core::cmp::min(bytes.len() - pos, MAX_FRAME - self.len);
            if take != 0 {
                self.buf[self.len..self.len + take].copy_from_slice(&bytes[pos..pos + take]);
                self.len += take;
                pos += take;
            }
            let before = self.len;
            self.scan();
            if take == 0 && self.len == before {
                // Unreachable by construction: `scan` either decides or discards a window
                // of MAX_FRAME, because no valid record is longer. Breaking rather than
                // spinning keeps that reasoning checkable instead of load-bearing on a
                // hang, and reports it through debug builds.
                debug_assert!(
                    false,
                    "decoder stalled with a full {}-byte window",
                    MAX_FRAME
                );
                break;
            }
        }
        pos
    }

    /// Take the oldest validated record not yet delivered, if any.
    ///
    /// The returned body borrows this decoder, so it stays valid only until the next
    /// [`push`](Decoder::push) or `next_record`: use each record before asking for the
    /// next. Draining to `None` after every push is the discipline that keeps delivery
    /// lossless; see [`RECORD_SLOTS`] for what happens when a reader does not.
    pub fn next_record(&mut self) -> Option<Record<'_>> {
        if self.head == self.tail {
            return None;
        }
        let slot = (self.head as usize) % RECORD_SLOTS;
        self.head += 1;
        Some(Record {
            level: self.queue_levels[slot],
            seq: self.queue_seqs[slot],
            t_ms: self.queue_times[slot],
            body: &self.queue_bodies[slot][..self.queue_body_lens[slot]],
        })
    }

    /// End the stream: abandon whatever window is still undecided.
    ///
    /// Required before reading [`stats`](Decoder::stats). The accounting law alone is
    /// not enough for a capture summary: a stream that ends mid-record, or with
    /// unframed terminal text after the last record, leaves those bytes sitting in
    /// [`buffered`](Decoder::buffered) — and printing `discarded_bytes = 0` there is
    /// precisely the lie this protocol exists to prevent. After `finish()` those bytes
    /// are counted, `buffered()` is zero, and the law still holds.
    pub fn finish(&mut self) {
        self.discard_front(self.len);
        self.in_candidate = false;
    }

    /// Bytes offered but not yet decided — the window still in play.
    pub fn buffered(&self) -> usize {
        self.len
    }

    /// Counters so far. Complete only after [`finish`](Decoder::finish).
    pub fn stats(&self) -> Stats {
        self.stats
    }

    #[cfg(test)]
    fn delivery_full_for_test(&self) -> bool {
        self.delivery_full()
    }

    /// Drop `n` leading bytes that never reached a validated record, charging them to
    /// `discarded_bytes`.
    fn discard_front(&mut self, n: usize) {
        self.stats.discarded_bytes += n as u64;
        self.consume_front(n);
    }

    /// Drop `n` leading bytes without charging them: either the bytes of a record that
    /// just validated, or a window already accounted for. Keeping this separate from
    /// [`discard_front`](Self::discard_front) is what makes the accounting law hold
    /// after every push rather than only at the end.
    fn consume_front(&mut self, n: usize) {
        debug_assert!(n <= self.len);
        self.buf.copy_within(n..self.len, 0);
        self.len -= n;
    }

    /// Run the two-state machine until nothing more can be decided.
    ///
    /// Returns whether anything left the window or changed state; `push` uses that to
    /// recognise the impossible full-window stall rather than looping on it.
    fn scan(&mut self) {
        loop {
            if !self.in_candidate {
                match find_byte(&self.buf[..self.len], b'~') {
                    Some(i) => {
                        if i > 0 {
                            // Inter-record junk: counted once, here, and only here.
                            self.discard_front(i);
                        }
                        self.in_candidate = true;
                        if self.armed_resync {
                            self.stats.resyncs += 1;
                            self.armed_resync = false;
                        }
                        continue;
                    }
                    None => {
                        if self.len >= MAX_FRAME {
                            // No start marker can have been missed: every record needs
                            // one and none is in a window this wide.
                            self.discard_front(self.len);
                        }
                        return;
                    }
                }
            }

            match self.examine() {
                Decision::NeedMore => return,
                Decision::Reject => {
                    self.stats.bad_frames += 1;
                    // Charge the forfeited start marker itself and nothing else: the
                    // rest of the window stays for the next scan.
                    self.discard_front(1);
                    self.in_candidate = false;
                    self.armed_resync = true;
                }
                Decision::Accept(frame) => {
                    self.stats.records += 1;
                    // Queue first: the body still lives in `buf`, and consuming shifts
                    // it away.
                    self.enqueue(frame);
                    self.consume_front(frame.consumed);
                    self.in_candidate = false;
                    if self.delivery_full() {
                        // Hand the caller room to drain before deciding anything else;
                        // `push` reports the rest of its input as untaken.
                        return;
                    }
                }
            }
        }
    }

    /// Decide the candidate at `buf[0]`, deriving every offset from `q` = index of CR.
    ///
    /// Order matters less than completeness: cheap structural checks come first so a
    /// corrupt span rarely reaches the CRC, but the decision is the same whichever
    /// order they run in.
    fn examine(&self) -> Decision {
        let buf = &self.buf[..self.len];

        // First complete CRLF at or after index 1. An unterminated CR at the very end
        // is not a decision point — it is the “need more bytes” case.
        let Some(q) = find_crlf(buf) else {
            // Over-length is the only give-up condition without a CRLF: nothing valid
            // is longer than MAX_FRAME, so a window this big is junk however it ends.
            return if self.len >= MAX_FRAME {
                Decision::Reject
            } else {
                Decision::NeedMore
            };
        };

        // `r` is `j - start` from the derivation table: MIN_CR_OFFSET + body_len.
        let r = q;
        if r < MIN_CR_OFFSET || r - MIN_CR_OFFSET > MAX_BODY {
            return Decision::Reject;
        }
        // `'*'` located FROM the terminator, never by searching forward: a body may
        // legitimately contain '*'.
        if buf[q - 5] != b'*' {
            return Decision::Reject;
        }
        let Some(crc_field) = parse_hex(&buf[q - 4..q]) else {
            return Decision::Reject;
        };
        if !matches!(buf[1], b'I' | b'W' | b'E' | b'D' | b'T') {
            return Decision::Reject;
        }
        if buf[2] != b' ' || buf[11] != b' ' || buf[20] != b' ' {
            return Decision::Reject;
        }
        let Some(seq) = parse_hex(&buf[3..11]) else {
            return Decision::Reject;
        };
        let Some(t_ms) = parse_decimal(&buf[12..20]) else {
            return Decision::Reject;
        };

        let body_end = q - 5;
        // Four hex digits cannot exceed u16, so widening the computed CRC is lossless.
        if u32::from(crc16_ccitt(&buf[1..body_end])) != crc_field {
            return Decision::Reject;
        }

        Decision::Accept(Accepted {
            level: buf[1],
            seq,
            t_ms,
            body_len: body_end - PREFIX_LEN,
            // Total bytes this record occupies: the relative CR offset plus CRLF.
            consumed: r + 2,
        })
    }
}

/// Outcome of examining one candidate start.
enum Decision {
    /// Undecidable on the bytes offered so far; keep everything.
    NeedMore,
    /// This start marker cannot become a valid record, now or with more bytes.
    Reject,
    Accept(Accepted),
}

/// A candidate that validated: framing and CRC both agreed.
#[derive(Clone, Copy)]
struct Accepted {
    level: u8,
    seq: u32,
    t_ms: u32,
    body_len: usize,
    consumed: usize,
}

impl Decoder {
    /// True when no more records can be accepted until the caller drains.
    fn delivery_full(&self) -> bool {
        self.tail - self.head == RECORD_SLOTS as u64
    }

    /// Copy a freshly validated record into the delivery queue.
    ///
    /// Called only when [`delivery_full`](Self::delivery_full) is false, which
    /// [`push`](Self::push) checks before taking any more input, so nothing is ever
    /// overwritten here.
    fn enqueue(&mut self, frame: Accepted) {
        debug_assert!(!self.delivery_full());
        let slot = (self.tail as usize) % RECORD_SLOTS;
        self.tail += 1;
        self.queue_levels[slot] = frame.level;
        self.queue_seqs[slot] = frame.seq;
        self.queue_times[slot] = frame.t_ms;
        self.queue_body_lens[slot] = frame.body_len;
        self.queue_bodies[slot][..frame.body_len]
            .copy_from_slice(&self.buf[PREFIX_LEN..PREFIX_LEN + frame.body_len]);
    }
}

fn find_byte(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&b| b == needle)
}

/// Index of the first complete CRLF at or after index 1, or `None` if the bytes
/// offered so far do not contain one (including the case where a trailing CR has not
/// yet been followed by its LF).
fn find_crlf(buf: &[u8]) -> Option<usize> {
    (1..buf.len().saturating_sub(1)).find(|&i| buf[i] == b'\r' && buf[i + 1] == b'\n')
}

/// Lowercase hex only: uppercase is not in the grammar, and accepting it would widen
/// what counts as a valid record beyond what the encoder can produce.
///
/// Crate-visible because `dump::BlockAssembler` parses the same grammar on the receiving side — a
/// second spelling of "valid hex" there is how an encoder and an assembler drift apart.
pub(crate) fn parse_hex(digits: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u32,
            b'a'..=b'f' => (byte - b'a' + 10) as u32,
            _ => return None,
        };
        value = (value << 4) | digit;
    }
    Some(value)
}

/// Same rule as [`parse_hex`], shared with `dump` for the same reason.
pub(crate) fn parse_decimal(digits: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

// ---------------------------------------------------------------------------
// Tests — host-only, default features, no hardware types
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::{vec, vec::Vec};

    /// Encode into a fresh buffer, zeroed so anything past `len` is visibly unused.
    fn encoded(level: Level, seq: u32, now_ms: u32, body: &[u8]) -> ([u8; MAX_FRAME], Encoded) {
        let mut out = [0u8; MAX_FRAME];
        let enc = encode(level, seq, now_ms, body, &mut out);
        (out, enc)
    }

    // ── CRC ───────────────────────────────────────────────────────────

    #[test]
    fn crc16_check_vector_is_0x29b1() {
        // The catalogue's `check` value for CRC-16/CCITT-FALSE. If a parameter drifts
        // this fails before any framing test has to interpret the damage.
        assert_eq!(crc16_ccitt(b"123456789"), 0x29b1);
    }

    #[test]
    fn crc16_of_empty_input_is_the_init_value() {
        assert_eq!(crc16_ccitt(b""), 0xFFFF);
    }

    // ── Golden frames ────────────────────────────────────────────────
    //
    // One #[test] per vector so a failure names the field that drifted, following the
    // `golden_cases!` rationale in crates/asperitas-cli/tests/golden_tests.rs.

    #[test]
    fn encode_golden_normal_record() {
        let (out, enc) = encoded(Level::Info, 0x42, 4567, b"ENC +1");
        assert_eq!(
            &out[..enc.len],
            b"~I 00000042 00004567 ENC +1*9c17\r\n",
            "normal record drifted"
        );
        assert_eq!(enc.len, 34);
        assert!(!enc.truncated);
    }

    #[test]
    fn encode_golden_empty_body_is_the_28_byte_overhead_proof() {
        let (out, enc) = encoded(Level::Debug, 0, 0, b"");
        assert_eq!(
            &out[..enc.len],
            b"~D 00000000 00000000 *91d4\r\n",
            "empty-body record drifted"
        );
        assert_eq!(enc.len, PREFIX_LEN + TRAILER_LEN);
        assert_eq!(enc.len, 28);
        assert!(!enc.truncated);
    }

    #[test]
    fn encode_golden_wraps_t_ms_and_passes_utf8_through() {
        // 0x9999_9999 = 2 576 980 377 ms → 76980377 after the modulo, and a 3-byte
        // UTF-8 checkmark that must arrive intact. One vector pins both, because the
        // modulo and the pass-through rule interact at the body boundary.
        let (out, enc) = encoded(
            Level::Warn,
            0xDEAD_BEEF,
            0x9999_9999,
            "knob r2=298 ✓".as_bytes(),
        );
        assert_eq!(
            &out[..enc.len],
            // "knob r2=298 ✓" — the checkmark as its three UTF-8 bytes.
            b"~W deadbeef 76980377 knob r2=298 \xe2\x9c\x93*b321\r\n",
            "t_ms modulo or UTF-8 pass-through drifted"
        );
        assert_eq!(enc.len, 43);
        assert!(!enc.truncated);
    }

    #[test]
    fn encode_golden_max_body_hits_the_frame_limit_exactly() {
        let (out, enc) = encoded(Level::Trace, 0, 0, &[b'a'; MAX_BODY]);
        assert_eq!(enc.len, MAX_FRAME);
        assert_eq!(enc.len, 228);
        assert!(!enc.truncated, "exactly MAX_BODY is not truncation");
        assert_eq!(
            &out[..PREFIX_LEN],
            b"~T 00000000 00000000 ",
            "prefix of a max-size record drifted"
        );
        assert_eq!(
            &out[enc.len - TRAILER_LEN..],
            b"*6c90\r\n",
            "CRC field drifted"
        );
    }

    #[test]
    fn encode_maps_every_level_to_its_wire_letter() {
        let cases = [
            (Level::Error, b'E'),
            (Level::Warn, b'W'),
            (Level::Info, b'I'),
            (Level::Debug, b'D'),
            (Level::Trace, b'T'),
        ];
        for (level, letter) in cases {
            let (out, enc) = encoded(level, 0, 0, b"");
            assert_eq!(out[1], letter, "{level} must map to '{}'", letter as char);
            assert_eq!(enc.len, 28);
        }
    }

    // ── Sanitisation and capping ─────────────────────────────────────

    #[test]
    fn sanitiser_replaces_control_bytes_but_keeps_printable_punctuation() {
        let (out, enc) = encoded(Level::Info, 1, 1, b"cr\r\nlf\x00\x7f~*|");
        // `~`, `*` and `|` surviving is deliberate (TASK-030 §3 sanitises only control
        // bytes) and the decoder's adversarial suite depends on it: the invariant the
        // parse proof needs is "no CR or LF", not "no tilde".
        assert_eq!(
            &out[PREFIX_LEN..PREFIX_LEN + 11],
            b"cr__lf__~*|",
            "sanitisation changed the wrong bytes"
        );
        assert_eq!(enc.len, PREFIX_LEN + 11 + TRAILER_LEN);
        assert!(!enc.truncated);
    }

    #[test]
    fn sanitiser_leaves_high_bytes_alone() {
        for b in 0x20u8..0x7F {
            assert_eq!(sanitize_byte(b), b, "printable byte {b:#04x} altered");
        }
        for b in 0x80u8..=0xFF {
            assert_eq!(sanitize_byte(b), b, "high byte {b:#04x} altered");
        }
        for b in 0u8..0x20 {
            assert_eq!(sanitize_byte(b), b'_', "control byte {b:#04x} not replaced");
        }
        assert_eq!(sanitize_byte(0x7F), b'_', "DEL not replaced");
    }

    #[test]
    fn over_long_body_is_capped_before_the_crc_and_reports_truncation() {
        let body: Vec<u8> = (0..300).map(|i| b'a' + (i % 26) as u8).collect();
        let (out, enc) = encoded(Level::Info, 7, 7, &body);

        assert!(enc.truncated, "300 > {MAX_BODY} must report truncation");
        assert_eq!(enc.len, MAX_FRAME, "capped record is a full-size frame");

        // Verify the shipped record against itself rather than trusting a literal:
        // the point of capping *before* the CRC is that the truncated record still
        // validates, so it must not be counted as a dropped record upstream.
        let len = enc.len;
        assert_eq!(out[len - 2..], b"\r\n"[..]);
        assert_eq!(out[len - TRAILER_LEN], b'*');
        let shipped_hex = core::str::from_utf8(&out[len - 6..len - 2]).unwrap();
        let shipped_crc =
            u16::from_str_radix(shipped_hex, 16).expect("shipped CRC digits must be lowercase hex");
        assert_eq!(
            shipped_crc,
            crc16_ccitt(&out[1..len - TRAILER_LEN]),
            "CRC was not computed over the capped body"
        );

        // And the boundary case: exactly MAX_BODY is not truncation.
        let (_, exact) = encoded(Level::Info, 7, 7, &body[..MAX_BODY]);
        assert!(
            !exact.truncated,
            "MAX_BODY exactly must not report truncation"
        );
        assert_eq!(exact.len, MAX_FRAME);
    }

    // ── Decoder: the invariants worth reading first ──────────────────

    /// A delivered record in a form tests can compare without borrowing the decoder.
    type Seen = (u8, u32, u32, Vec<u8>);

    /// Feed `bytes` in chunks of `sizes`, returning every record the decoder yields.
    ///
    /// Chunking is expressed as a count of chunk sizes rather than as bytes so a test
    /// can say “ten pushes” without caring where the boundaries fall.
    fn decode_in_chunks(bytes: &[u8], sizes: &[usize]) -> Vec<Seen> {
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        let mut pos = 0usize;
        while pos < bytes.len() {
            let take = sizes[pos % sizes.len()].max(1);
            let end = (pos + take).min(bytes.len());
            let mut off = pos;
            while off < end {
                off += decoder.push(&bytes[off..end]);
                while let Some(r) = decoder.next_record() {
                    out.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
                }
            }
            pos = end;
        }
        decoder.finish();
        out
    }

    fn decode_whole(bytes: &[u8]) -> Vec<Seen> {
        decode_in_chunks(bytes, &[usize::MAX / 2])
    }

    /// Push a whole stream and return the records plus the final counters.
    fn decode_stats(bytes: &[u8]) -> (Vec<Seen>, Stats) {
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        let mut off = 0usize;
        while off < bytes.len() {
            off += decoder.push(&bytes[off..]);
            while let Some(r) = decoder.next_record() {
                out.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
            }
        }
        decoder.finish();
        (out, decoder.stats())
    }

    #[test]
    fn decoder_returns_the_fields_a_record_was_encoded_with() {
        let (out, enc) = encoded(Level::Warn, 0xDEAD_BEEF, 987_654, b"knob r1=0.42 ~*|");
        let (records, stats) = decode_stats(&out[..enc.len]);
        assert_eq!(
            records,
            vec![(b'W', 0xDEAD_BEEF, 987_654, b"knob r1=0.42 ~*|".to_vec())],
            "decoded fields must match what went in"
        );
        assert_eq!(
            stats,
            Stats {
                records: 1,
                bad_frames: 0,
                resyncs: 0,
                discarded_bytes: 0
            },
            "a clean capture must report a clean capture"
        );
    }

    #[test]
    fn decoder_holds_no_more_than_one_frame_of_undecided_bytes() {
        // A capture of pure junk is the case that would otherwise grow a reader without
        // bound, since there is no delimiter to stop it at.
        let mut decoder = Decoder::new();
        for i in 0..10_000u32 {
            decoder.push(&[b'x', b'~', (i as u8) & 0x7F, b'\r', b'\n', 0x00, 0xFF, b'*']);
            while decoder.next_record().is_some() {}
            assert!(
                decoder.buffered() <= MAX_FRAME,
                "buffered {} exceeds MAX_FRAME",
                decoder.buffered()
            );
        }
    }

    #[test]
    fn finish_moves_the_pending_window_into_discarded_bytes() {
        // The lie TASK-030 exists to prevent: a capture that ends mid-record must not
        // summarise as zero loss.
        let (out, enc) = encoded(Level::Info, 1, 1, b"tail-truncated");
        let whole = &out[..enc.len];
        let truncated = &whole[..whole.len() - 4];

        let mut decoder = Decoder::new();
        decoder.push(truncated);
        assert_eq!(decoder.stats().discarded_bytes, 0, "not decided yet");
        assert_eq!(decoder.buffered(), truncated.len());

        decoder.finish();
        assert_eq!(decoder.buffered(), 0);
        assert_eq!(decoder.stats().discarded_bytes, truncated.len() as u64);
        assert_eq!(decoder.stats().records, 0);
    }

    #[test]
    fn one_byte_at_a_time_decodes_identically_to_one_push() {
        let (out, enc) = encoded(Level::Info, 5, 5, b"chunking must not matter");
        let stream = &out[..enc.len];
        assert_eq!(decode_in_chunks(stream, &[1]), decode_whole(stream));
    }

    /// Pump `bytes` through the decoder using the loop its own documentation prescribes,
    /// so the test exercises the contract a reader is actually given.
    fn pump(decoder: &mut Decoder, bytes: &[u8]) -> Vec<Seen> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off < bytes.len() {
            let before = off;
            off += decoder.push(&bytes[off..]);
            debug_assert!(
                off > before || decoder.delivery_full_for_test(),
                "no progress"
            );
            while let Some(r) = decoder.next_record() {
                out.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
            }
        }
        out
    }

    #[test]
    fn a_multi_record_chunk_reaches_a_draining_reader_with_no_loss() {
        // Records are as short as 28 bytes, so one chunk routinely validates many of
        // them. This is the case a single delivery slot cannot serve: it would have to
        // either drop the input or overwrite undelivered records.
        let mut stream = Vec::new();
        for i in 0..200u32 {
            let (bytes, enc) = encoded(Level::Info, i, 1_000 + i, b"x");
            stream.extend_from_slice(&bytes[..enc.len]);
        }

        let mut decoder = Decoder::new();
        let records = pump(&mut decoder, &stream);
        decoder.finish();

        assert_eq!(records.len(), 200, "every record in the chunk must arrive");
        assert_eq!(
            records.first().map(|r| r.1),
            Some(0),
            "and in the order written"
        );
        assert_eq!(records.last().map(|r| r.1), Some(199));
        let stats = decoder.stats();
        assert_eq!(
            (stats.records, stats.bad_frames, stats.discarded_bytes),
            (200, 0, 0)
        );
    }

    #[test]
    fn push_reports_a_short_count_rather_than_losing_records() {
        // A reader that never drains while pushing must still lose nothing: the unread
        // records wait in the queue, the untaken bytes stay the caller's, and the
        // capture summary stays clean.
        let mut stream = Vec::new();
        for i in 0..200u32 {
            let (bytes, enc) = encoded(Level::Info, i, 1_000 + i, b"x");
            stream.extend_from_slice(&bytes[..enc.len]);
        }

        let mut decoder = Decoder::new();
        let taken = decoder.push(&stream);
        assert!(
            taken < stream.len(),
            "one push must refuse what it cannot hold"
        );
        assert_eq!(decoder.stats().records as usize, RECORD_SLOTS);

        let mut records = Vec::new();
        while let Some(r) = decoder.next_record() {
            records.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
        }
        records.extend(pump(&mut decoder, &stream[taken..]));
        decoder.finish();

        assert_eq!(
            records.len(),
            200,
            "backpressure must cost latency, not data"
        );
        assert_eq!(
            records.iter().map(|r| r.1).collect::<Vec<u32>>(),
            (0u32..200).collect::<Vec<u32>>(),
            "in sequence order, with none missing"
        );
        assert_eq!(
            decoder.stats().discarded_bytes,
            0,
            "nothing may be counted lost"
        );
    }

    // ── write_whole ──────────────────────────────────────────────────

    #[test]
    fn write_whole_refuses_a_frame_that_does_not_fit_without_writing_anything() {
        let frame = *b"~I 00000000 00000000 no-fit*0000\r\n";
        let mut calls = 0usize;
        let outcome = write_whole(&frame, frame.len() - 1, |chunk| {
            calls += 1;
            Some(chunk.len())
        });
        assert_eq!(
            outcome,
            WriteOutcome::RefusedForSpace,
            "must refuse when free capacity is one byte short"
        );
        assert_eq!(calls, 0, "refusal must not touch the sink at all");

        // Zero-length frames are legal (an empty body still frames) and always fit.
        let mut called = false;
        assert_eq!(
            write_whole(&[], 0, |_| {
                called = true;
                Some(0)
            }),
            WriteOutcome::Committed
        );
        assert!(!called, "an empty frame commits without calling the sink");
    }

    #[test]
    fn write_whole_accepts_only_after_every_byte_reaches_the_sink() {
        let frame = *b"~I 00000000 00000000 whole*abcd\r\n";
        let mut got = Vec::new();
        let outcome = write_whole(&frame, frame.len(), |chunk| {
            // One byte at a time: the worst-case sink the loop can face.
            got.extend_from_slice(&chunk[..1]);
            Some(1)
        });
        assert_eq!(outcome, WriteOutcome::Committed);
        assert_eq!(got, frame.to_vec());
    }

    /// A sink that accepts some bytes and then stops accepting them has broken
    /// `write_whole`'s precondition (the caller holds the lock and already confirmed the
    /// frame fits), so the call must report rather than retry the identical write forever.
    ///
    /// What is asserted here is the *value* the callers branch on. The panic itself now
    /// lives in the caller — `commit_records` in `lib.rs` — which does not even link on
    /// host, so this test must not try to observe the crash. The history is load-bearing:
    /// the earlier `debug_assert!` was rejected because it passed in debug and *failed to*
    /// fire in release, where the loop spun silently inside the caller's lock; the failure
    /// must stay loud in both profiles, which is why the caller panics outright rather
    /// than asserting.
    #[test]
    fn write_whole_reports_stalled_when_the_sink_stalls_after_the_precheck() {
        let frame = *b"~I 00000000 00000000 stalled*abcd\r\n";
        let mut calls = 0usize;

        let outcome = write_whole(&frame, frame.len(), |chunk| {
            calls += 1;
            // Round one accepts a prefix, so the loop is genuinely mid-frame with bytes
            // already committed; round two reports success with zero progress.
            if calls == 1 {
                Some(chunk.len() / 2)
            } else {
                Some(0)
            }
        });

        assert_eq!(outcome, WriteOutcome::Stalled);
        assert_eq!(
            calls, 2,
            "the sink must stop mid-frame, not on first contact"
        );
    }

    /// A real `embassy_sync` ring buffer, driven across the wrap condition.
    ///
    /// `NoopRawMutex` for two reasons: with no `critical-section` impl registered for
    /// host, `CriticalSectionRawMutex` is an undefined symbol at *link* time, and the
    /// `std` fallback is not re-entrant. It also means the pipe cannot live in a
    /// `static` here — `NoopRawMutex` is `!Sync` (`PhantomData<*mut ()>`), so the
    /// buffer is a local borrowed for the test's duration instead. On target the
    /// static form works because `CriticalSectionRawMutex` is `Sync`.
    fn pipe512() -> embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, 512> {
        embassy_sync::pipe::Pipe::new()
    }

    /// Commit through the real pipe, mirroring what the device call site will do:
    /// `|c| LOG_PIPE.try_write(c).ok()`.
    fn commit(
        pipe: &embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, 512>,
        frame: &[u8],
    ) -> bool {
        matches!(
            write_whole(frame, pipe.free_capacity(), |chunk| {
                pipe.try_write(chunk).ok()
            }),
            WriteOutcome::Committed
        )
    }

    /// Advance both cursors `at` bytes into the backing array, leaving the ring empty
    /// by occupancy — the state in which a wrap short-write happens.
    fn park_cursors_at(
        pipe: &embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, 512>,
        at: usize,
    ) {
        let filler = vec![b'f'; at];
        assert_eq!(pipe.try_write(&filler).ok(), Some(at));
        let mut drained = vec![0u8; at];
        assert_eq!(pipe.try_read(&mut drained).ok(), Some(at));
        assert_eq!(
            pipe.free_capacity(),
            512,
            "ring must report itself empty before the wrap test"
        );
    }

    #[test]
    fn a_single_try_write_short_writes_at_the_ring_wrap_even_when_empty() {
        // This pins the embassy-sync behaviour that makes the loop in `write_whole`
        // load-bearing rather than defensive: `RingBuffer::push_buf` returns only the
        // contiguous run to the end of the backing array, while `free_capacity()`
        // reports total free. A "check capacity, then write once" fix therefore still
        // truncates records — the bug this ticket replaces.
        let pipe = pipe512();
        park_cursors_at(&pipe, 400);

        let probe = [b'z'; 200];
        assert_eq!(
            pipe.try_write(&probe).ok(),
            Some(112),
            "expected the 512-400 contiguous run, not the requested 200"
        );
    }

    #[test]
    fn write_whole_commits_a_full_record_across_the_ring_wrap() {
        let pipe = pipe512();
        park_cursors_at(&pipe, 400);

        // Exactly 200 bytes on the wire: 21 prefix + 172 body + 7 trailer.
        let (out, enc) = encoded(Level::Info, 0xABCD, 12_345, &[b'a'; MAX_BODY - 28]);
        assert_eq!(enc.len, 200);
        let frame = &out[..enc.len];

        assert!(
            commit(&pipe, frame),
            "a 200-byte frame must commit into a ring reporting 512 free"
        );

        // Byte-exact read-back, however the pipe hands the bytes out.
        let mut got = Vec::with_capacity(frame.len());
        let mut chunk = [0u8; 64];
        while got.len() < frame.len() {
            let n = pipe.try_read(&mut chunk).ok().unwrap_or(0);
            assert_ne!(
                n,
                0,
                "pipe ran dry with {} of {} bytes read",
                got.len(),
                frame.len()
            );
            got.extend_from_slice(&chunk[..n]);
        }
        assert_eq!(got, frame.to_vec(), "committed frame is not what came back");
    }

    #[test]
    fn write_whole_rounds_are_byte_exact_under_randomized_interleaving() {
        // Ported from the planning probe /tmp/pipecheck2, which measured
        // commits=19799 drops=201 bytes_expected=2461609 bytes_got=2461609.
        // The assertion is on bytes, not counts: whatever the producer committed must
        // arrive from the consumer contiguously, in order, with no partial record ever
        // observable — across every ring wrap along the way.
        const CAP: usize = 512;
        const ROUNDS: usize = 20_000;

        let pipe = pipe512();
        let mut rng = XorShift::new(0x1234_5678);

        let mut expected: Vec<u8> = Vec::new();
        let mut got: Vec<u8> = Vec::new();
        let mut buf = [0u8; CAP];
        let mut commits = 0usize;
        let mut drops = 0usize;

        for round in 0..ROUNDS {
            let n = 20 + rng.next() % 210;
            let frame: Vec<u8> = (0..n).map(|i| b'A' + (i % 26) as u8).collect();
            let free_before = pipe.free_capacity();

            if commit(&pipe, &frame) {
                commits += 1;
                expected.extend_from_slice(&frame);
            } else {
                drops += 1;
                assert!(
                    frame.len() > free_before,
                    "round {round}: refused a {}-byte frame that fit in {free_before} free bytes",
                    frame.len()
                );
            }

            // Consumer: drain a random amount, like the USB task does.
            let want = 1 + rng.next() % CAP;
            if let Ok(n) = pipe.try_read(&mut buf[..want]) {
                got.extend_from_slice(&buf[..n]);
            }
        }

        // Drain whatever is left, then compare streams byte-for-byte.
        while let Ok(n) = pipe.try_read(&mut buf) {
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
        }

        assert_eq!(
            expected, got,
            "stream is not an exact concatenation of committed frames (commits={commits} drops={drops})"
        );
        assert!(commits > 0 && drops > 0, "test exercised neither path");
    }

    /// Deterministic RNG so the randomized rounds replay identically on every run.
    struct XorShift(u32);

    impl XorShift {
        fn new(seed: u32) -> Self {
            Self(seed)
        }
        fn next(&mut self) -> usize {
            // Numerical Recipes LCG step, matching the planning probe so the recorded
            // commits/drops split stays comparable.
            self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (self.0 >> 16) as usize
        }
    }
}
