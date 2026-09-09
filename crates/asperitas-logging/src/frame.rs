//! Console protocol v1 — every log record carries its own framing and checksum.
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
//!
//! Continuity is the decoder's and the reader's business (`seq`, plus the `BOOT`
//! record that distinguishes a restart from loss); this module's business is that a
//! record which validates is byte-identical to one that was written.
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
fn write_hex(value: u32, dst: &mut [u8]) {
    debug_assert!(dst.len() <= 8, "hex field wider than a u32");
    let n = dst.len();
    for (i, slot) in dst.iter_mut().enumerate() {
        let shift = 4 * (n - 1 - i);
        *slot = HEX_DIGITS[((value >> shift) & 0xF) as usize];
    }
}

/// Render `value` as exactly `dst.len()` decimal digits, zero-padded. The caller is
/// responsible for fitting (see [`T_MS_WRAP`]).
fn write_decimal(value: u32, dst: &mut [u8]) {
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

/// Commit `frame` to a sink, or refuse it entirely. Exactly two outcomes:
///
/// - `false` — the frame did not fit in `free_capacity`, and `write` was called
///   **zero** times. Nothing reached the sink, so nothing needs unwinding.
/// - `true` — every byte of `frame` is in the sink, contiguously and in order.
///
/// There is no third outcome and no partial-write count on purpose: a half-written
/// record is precisely the failure mode framing exists to expose, so the API refuses
/// to express it. Reserve/commit-with-discard is the same vocabulary Linux's ring
/// buffer, printk's `prb_reserve`/`prb_commit` and bitdrift's reserve/commit buffer
/// use.
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
/// Given the pre-check plus the caller holding its lock, a stall mid-loop
/// (`None`/`Some(0)` after progress) means the sink broke its contract. Debug builds
/// assert loudly; release builds keep trying rather than reporting success early. If
/// an embassy change ever made it genuinely possible, the leftover fragment is caught
/// by the reader's CRC and counted as an integrity failure — degraded, but it cannot
/// masquerade as data, which is what v1 buys.
pub fn write_whole(
    frame: &[u8],
    free_capacity: usize,
    mut write: impl FnMut(&[u8]) -> Option<usize>,
) -> bool {
    if frame.len() > free_capacity {
        return false;
    }

    let mut written = 0usize;
    while written < frame.len() {
        match write(&frame[written..]) {
            Some(n) if n > 0 => written += n,
            stalled => {
                debug_assert!(
                    false,
                    "sink stalled after the capacity pre-check passed: {stalled:?}"
                );
            }
        }
    }
    true
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

    // ── write_whole ──────────────────────────────────────────────────

    #[test]
    fn write_whole_refuses_a_frame_that_does_not_fit_without_writing_anything() {
        let frame = *b"~I 00000000 00000000 no-fit*0000\r\n";
        let mut calls = 0usize;
        let ok = write_whole(&frame, frame.len() - 1, |chunk| {
            calls += 1;
            Some(chunk.len())
        });
        assert!(!ok, "must refuse when free capacity is one byte short");
        assert_eq!(calls, 0, "refusal must not touch the sink at all");

        // Zero-length frames are legal (an empty body still frames) and always fit.
        let mut called = false;
        assert!(write_whole(&[], 0, |_| {
            called = true;
            Some(0)
        }));
        assert!(!called, "an empty frame commits without calling the sink");
    }

    #[test]
    fn write_whole_accepts_only_after_every_byte_reaches_the_sink() {
        let frame = *b"~I 00000000 00000000 whole*abcd\r\n";
        let mut got = Vec::new();
        let ok = write_whole(&frame, frame.len(), |chunk| {
            // One byte at a time: the worst-case sink the loop can face.
            got.extend_from_slice(&chunk[..1]);
            Some(1)
        });
        assert!(ok);
        assert_eq!(got, frame.to_vec());
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
        write_whole(frame, pipe.free_capacity(), |chunk| {
            pipe.try_write(chunk).ok()
        })
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
