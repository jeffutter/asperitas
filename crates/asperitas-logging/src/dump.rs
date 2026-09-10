//! Audio-dump payload layer — the binary-to-text half of getting captured samples off the device.
//!
//! Captured audio has to travel through the console transport in [`crate::frame`], which is
//! printable-ASCII text: [`crate::frame::sanitize_byte`] replaces every control byte, so raw sample
//! bytes cannot go near it. This module supplies the codec that makes them fit — RFC 4648 standard
//! base64, written for `no_std`, allocation-free, and strict enough that silent corruption cannot
//! survive a round trip.
//!
//! On top of the codec sits the record grammar that carries these payloads — `AUDIO` and `AUDEND`
//! bodies, the chunk geometry derived from them, and one composite entry point per record kind — so
//! the efficiency figures quoted in documentation are the ones the encoder produces, not the ones a
//! plan hoped for. See *Record grammar* below.
//!
//! The last layer is the other direction: [`BlockAssembler`], which consumes those records on the host
//! and reassembles whole blocks of PCM, refusing any block whose completeness it cannot prove from
//! `n_of_n` sequence plus a block checksum. The format only earns the words “self-verifying” because
//! something acts on that proof — see *Host-side block assembly* near the end of this module.
//!
//! # Why this module is not behind `log-usb`
//!
//! Ungated for the same reason as [`crate::frame`]: it is pure byte arithmetic whose
//! equivalence with a reference implementation is proven on the host, where the oracle can
//! be a dev-dependency.
//!
//! # Why base64 and not something denser
//!
//! Every candidate has to survive `sanitize_byte` unchanged, and the transport reserves bytes for
//! its own protocol:
//!
//! | encoding | raw per 4 wire bytes | verdict |
//! |---|---|---|
//! | hex | 2 | survives sanitisation, but spends 50% more wire than base64 for nothing gained |
//! | Ascii85 / Z85 | ~5 | denser, but those alphabets contain `~` — the record start marker — and `<>`, which TASK-032 reserves host→device. A corrupted body could then be reassembled into a fake record boundary |
//! | **base64** | 3 | alphabet is `[A-Za-z0-9+/]` plus `=`: all printable, none reserved, no control bytes, no `~` |
//!
//! Base64 loses the density contest and wins the safety one. The arithmetic deciding how much
//! payload fits in a frame lives with the grammar, not here.
//!
//! # Strictness is a corruption detector, not a style preference
//!
//! `sanitize_byte` passes printable ASCII through untouched, so a bit-flip inside a payload leaves
//! behind a string that still looks entirely legal. The decoder's job is therefore to reject
//! everything the reference implementation rejects, including what a permissive decoder waves
//! through:
//!
//! - **Padding is mandatory.** `"AB="` and `"A"` are errors, not things to repair.
//! - **`=` may only close the string.** At most two, only in the final group, only in its last two
//!   positions, contiguous with the end.
//! - **Nothing follows padding**, and no whitespace appears anywhere — the transport never inserts
//!   it inside a body, so its presence means these are not the bytes that were sent.
//! - **Ignored bits must be zero.** In `"xy=="` the low 4 bits of `y`, and in `"xyz="` the low 2
//!   bits of `z`, carry no data. A permissive decoder discards them and decodes happily, which
//!   would let a corrupted final symbol whose error lands entirely in those dead bits pass
//!   unnoticed — the one hole that is otherwise invisible on the wire.
//!
//! Those rules were measured against `base64 0.23.1` (`STANDARD`) rather than assumed.
//! `tests/console_dump.rs` asserts accept/reject agreement and decoded-byte equality over random
//! inputs, mutated inputs, and an exhaustive sweep of every possible final symbol for each tail
//! length. Error *variants* are this module's own and are not expected to mirror the reference's
//! wording; only the decision and the bytes are comparable.
//!
//! # Record grammar (normative — copy from here, not from a plan)
//!
//! Both records ride the v1 framing in [`crate::frame`] untouched, so what follows describes bodies
//! only. `level`, `seq` and `t_ms` belong to the frame; nothing here reads a clock or a counter.
//!
//! ```text
//! AUDIO blk=<4 hex> n=<2 hex> c=<2 hex> d=<base64>
//! AUDEND blk=<4 hex> n=<2 hex> bytes=<5 dec> crc16=<4 hex>
//! ```
//!
//! | field | width | alphabet | meaning |
//! |---|---|---|---|
//! | `blk` | 4 | lowercase hex | Block id. Wraps every 65,536 blocks; ordering across runs is the frame `seq`'s job, not this field's. |
//! | `n` | 2 | lowercase hex | Chunks in this block, `1..=255`. Two digits hold no representation for 256, so a longer run starts another block. |
//! | `c` | 2 | lowercase hex | This chunk's zero-based index, `c < n`. |
//! | `d` | `4·⌈len/3⌉` | `[A-Za-z0-9+/]`, then mandatory `=` | Canonical base64 of the chunk's raw bytes: ≤ 172 characters ⇒ ≤ **129** raw bytes. |
//! | `bytes` | 5, fixed | decimal | Raw bytes in the reassembled block. Fixed width like the frame's `t_ms`, so a body's shape never depends on a value. |
//! | `crc16` | 4 | lowercase hex | CRC-16/CCITT-FALSE over the **raw concatenated block bytes in `c` order** — the samples themselves, never the base64 text and never any framing byte. |
//!
//! Sizes fall out of [`MAX_BODY`](crate::frame::MAX_BODY), which bounds the body *including* keys:
//!
//! | record | body | frame | raw payload | useful fraction |
//! |---|---|---|---|---|
//! | `AUDIO`, full chunk | 199 | 227 | 129 | 129 / 227 = **0.568** |
//! | `AUDIO`, final chunk | 27 + `4·⌈len/3⌉` | 28 + that | 1…129 | lower, by padding |
//! | `AUDEND` | 43 | 71 | — | — |
//!
//! Mono 16-bit capture at 96,000 B/s therefore costs 745 records/s ≈ 169 kB/s of console traffic
//! (`tests/console_dump.rs::published_efficiency_matches_the_encoder` recomputes all of it from these
//! constants and fails if the two disagree).
//!
//! **Why the keys are abbreviations.** Descriptive names (`block_index=/n_of_n=/chunk_i=`) spend 18
//! body bytes on letters and land at 111 raw bytes, 0.487 useful; the 150 raw bytes that layout
//! assumed were reachable fit in no header at all. Dropping keys entirely would buy 6 bytes (4.6%)
//! and cost whoever greps a cold capture everything the names say. Payload rounds down to whole
//! 4-character groups, so *any* header between 25 and 28 bytes yields the same 129 — the letters are
//! free. Full budget table and rejected alternatives: TASK-038.02's Implementation Notes.
//!
//! **Sample bytes cannot be mangled by framing**, because they are base64 before they are framed: the
//! alphabet is printable ASCII, so [`crate::frame::sanitize_byte`] has nothing to substitute. That is
//! a structural property rather than a lucky accident, and
//! `tests/console_dump.rs::round_trips_through_the_real_decoder` exercises it with payloads holding
//! every byte a substitution would target.
//!
//! **Chunking recommendation for the device writer (TASK-038.03): 64 chunks per block** — 8,256 raw
//! bytes ≈ 86 ms of mono 16-bit capture. Small enough that one lost block costs 86 ms of measurement
//! rather than the ring, and well inside the 255 the `n` field can name.

use log::Level;

use crate::frame::{
    self, check_decimal_fits, check_hex_width, crc16_ccitt, parse_decimal, parse_hex,
    write_decimal, write_hex, Encoded, MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN,
};

/// The RFC 4648 §4 standard alphabet, in order. Index with a 6-bit value.
pub const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Marker standing in for the input bytes a padded group did not carry.
const PAD: u8 = b'=';

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why an encoding attempt could not write its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// `out` holds fewer than `need` bytes; nothing was written.
    OutputTooSmall { need: usize },
}

/// Why a base64 string was refused. Every variant names the offending offset so a caller can
/// report *where* a capture went wrong rather than only *that* it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Input length is not a whole number of 4-character groups.
    Length(usize),
    /// `byte` at `at` is not an alphabet symbol.
    Char { at: usize, byte: u8 },
    /// `=` at `at` sits where the grammar forbids it: inside a group, split from the end, or
    /// outnumbering the bytes it stands for.
    Padding { at: usize },
    /// The final symbol at `at` is a legitimate character whose unused low bits are set, so the
    /// string is not the canonical encoding of any byte sequence.
    TrailingBits { at: usize },
    /// `out` holds fewer than `need` decoded bytes; nothing was written.
    OutputTooSmall { need: usize },
}

// ---------------------------------------------------------------------------
// Inverse alphabet — built at compile time so decoding is a table lookup
// ---------------------------------------------------------------------------

/// Sentinel for "not part of the encoding".
const INVALID: i8 = -1;
/// Sentinel for the padding character, usable only where [`tail_shape`] permits it.
const PAD_SLOT: i8 = -2;

/// Byte → 6-bit value, `-1` outside the alphabet, `-2` for `=`.
///
/// A table rather than a chain of range comparisons because the alternative is six branches per
/// symbol on a path that runs once per payload byte, and because validity falls out of the lookup:
/// indexing a 64-entry symbol table with an unvalidated byte is how a corrupted character becomes
/// a silently wrong value instead of an error.
const DECODE_TABLE: [i8; 256] = build_decode_table();

const fn build_decode_table() -> [i8; 256] {
    let mut table = [INVALID; 256];
    let mut symbol = 0usize;
    while symbol < 64 {
        table[B64_ALPHABET[symbol] as usize] = symbol as i8;
        symbol += 1;
    }
    table[PAD as usize] = PAD_SLOT;
    table
}

// ---------------------------------------------------------------------------
// Size arithmetic
// ---------------------------------------------------------------------------

/// Exact encoded length of `raw` input bytes: one 4-character group per 3 input bytes, rounded up
/// so a partial group still gets a full group with padding.
///
/// Clamps to `usize::MAX` instead of wrapping. Callers size buffers with this, and a wrapped value
/// would hand them room that is too small while reporting success.
pub const fn encoded_len(raw: usize) -> usize {
    let groups = if raw.is_multiple_of(3) {
        raw / 3
    } else {
        raw / 3 + 1
    };
    if groups > usize::MAX / 4 {
        usize::MAX
    } else {
        groups * 4
    }
}

/// Largest raw byte count `b64_chars` characters can encode — the room a caller needs before
/// calling [`decode`]. Only whole groups carry data, so this rounds down to match what [`decode`]
/// will actually accept.
pub const fn max_raw_for(b64_chars: usize) -> usize {
    let groups = b64_chars / 4;
    if groups > usize::MAX / 3 {
        usize::MAX
    } else {
        groups * 3
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Encode `raw` into `out`, returning the number of characters written.
///
/// Output is always canonical and always padded, hence byte-identical to `base64`'s `STANDARD`
/// engine. Each whole group accumulates a 24-bit window and emits four symbols; a short output
/// buffer is reported before anything is written, leaving `out` untouched.
pub fn encode(raw: &[u8], out: &mut [u8]) -> Result<usize, EncodeError> {
    let need = encoded_len(raw.len());
    if out.len() < need {
        return Err(EncodeError::OutputTooSmall { need });
    }

    let mut written = 0usize;
    let mut groups = raw.chunks_exact(3);
    for group in &mut groups {
        let window = u32::from(group[0]) << 16 | u32::from(group[1]) << 8 | u32::from(group[2]);
        out[written] = B64_ALPHABET[(window >> 18) as usize & 0x3F];
        out[written + 1] = B64_ALPHABET[(window >> 12) as usize & 0x3F];
        out[written + 2] = B64_ALPHABET[(window >> 6) as usize & 0x3F];
        out[written + 3] = B64_ALPHABET[window as usize & 0x3F];
        written += 4;
    }

    // At most one tail step: 1 raw byte encodes as "xy==", 2 as "xyz=". Both pad positions left
    // unused are filled with `PAD`, which is what keeps the output canonical.
    let tail = groups.remainder();
    if !tail.is_empty() {
        let mut window = u32::from(tail[0]) << 16;
        if tail.len() > 1 {
            window |= u32::from(tail[1]) << 8;
        }
        out[written] = B64_ALPHABET[(window >> 18) as usize & 0x3F];
        out[written + 1] = B64_ALPHABET[(window >> 12) as usize & 0x3F];
        out[written + 2] = if tail.len() > 1 {
            B64_ALPHABET[(window >> 6) as usize & 0x3F]
        } else {
            PAD
        };
        out[written + 3] = PAD;
        written += 4;
    }

    debug_assert_eq!(written, need, "encoded length disagrees with itself");
    Ok(written)
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Decode a base64 string into `out`, returning the number of raw bytes written.
///
/// Strict in every direction described at the top of this module: a whole number of groups,
/// canonical trailing padding only, no byte outside the alphabet, and zeroed ignored bits in the
/// final symbol. An undersized output buffer is reported before anything is written.
pub fn decode(b64: &[u8], out: &mut [u8]) -> Result<usize, DecodeError> {
    if !b64.len().is_multiple_of(4) {
        return Err(DecodeError::Length(b64.len()));
    }

    let groups = b64.len() / 4;
    let (tail_symbols, pad_count) = tail_shape(b64)?;
    let need = max_raw_for(b64.len()) - pad_count;
    if out.len() < need {
        return Err(DecodeError::OutputTooSmall { need });
    }

    let mut written = 0usize;
    for group_index in 0..groups.saturating_sub(1) {
        let window = window_of(&b64[group_index * 4..group_index * 4 + 4], group_index * 4)?;
        out[written] = (window >> 16) as u8;
        out[written + 1] = (window >> 8) as u8;
        out[written + 2] = window as u8;
        written += 3;
    }

    // The final group is the only one allowed padding, and the only one whose symbols can carry
    // ignored bits: six bits per symbol against eight bits per output byte leaves `8 - 2 * symbols`
    // bits over — none for four symbols, two for three, four for two.
    if groups > 0 {
        let at = (groups - 1) * 4;
        let window = window_of(&b64[at..at + tail_symbols], at)?;
        let spare_bits = 8 - 2 * tail_symbols;
        if window & ((1u32 << spare_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                at: at + tail_symbols - 1,
            });
        }
        // Shifting the dead bits off the bottom leaves exactly eight bits per remaining symbol.
        let payload = window >> spare_bits;
        for shift in (0..tail_symbols - 1).rev() {
            out[written] = (payload >> (shift * 8)) as u8;
            written += 1;
        }
    }

    debug_assert_eq!(written, need, "decoded length disagrees with itself");
    Ok(written)
}

/// Shape of the final group: how many of its four characters are real symbols, and therefore how
/// many input bytes they stand in for.
///
/// This is the single place deciding whether padding is legal, so the rules cannot drift apart
/// between validation and decoding. A group of all-padding (`"===="`) and a group padded too early
/// to represent the bytes it claims (`"A==="`) are both refusals: two pads is the most any group
/// can mean, because three symbols already yield two bytes.
fn tail_shape(b64: &[u8]) -> Result<(usize, usize), DecodeError> {
    if b64.is_empty() {
        return Ok((0, 0));
    }

    let at = b64.len() - 4;
    let tail = &b64[at..];
    let Some(first_pad) = tail.iter().position(|byte| *byte == PAD) else {
        return Ok((4, 0));
    };

    // Padding is a suffix of the string occupying at most the last two positions: `first_pad >= 2`
    // bounds the count, and every character from there to the end has to be padding. That rejects
    // `"QU=JDRA="` (pad mid-string), `"QUJDRA=B"` (pad not last), and `"AAAA=A=="` (pad split).
    if first_pad < 2 || tail[first_pad..].iter().any(|byte| *byte != PAD) {
        return Err(DecodeError::Padding { at: at + first_pad });
    }

    Ok((first_pad, 4 - first_pad))
}

/// Six-bit values of `group`, packed into a right-aligned window of `6 * group.len()` bits.
/// `at` is where `group` starts in the string being decoded, so errors name an offset the caller
/// can point at in its own buffer rather than one relative to a slice it cannot see.
///
/// Callers shift out the ignored bits themselves; keeping that decision here would force this
/// function to know whether it is looking at a whole group or a padded tail.
fn window_of(group: &[u8], at: usize) -> Result<u32, DecodeError> {
    debug_assert!(
        (2..=4).contains(&group.len()),
        "a group carries two to four symbols"
    );
    let mut window = 0u32;
    for (offset, byte) in group.iter().enumerate() {
        let slot = DECODE_TABLE[usize::from(*byte)];
        if slot < 0 {
            return Err(if slot == PAD_SLOT {
                DecodeError::Padding { at: at + offset }
            } else {
                DecodeError::Char {
                    at: at + offset,
                    byte: *byte,
                }
            });
        }
        window = window << 6 | slot as u32;
    }
    Ok(window)
}

// ---------------------------------------------------------------------------
// Record grammar — geometry derived from the bytes the writer actually emits
// ---------------------------------------------------------------------------

/// `AUDIO blk=` — opens a chunk body.
///
/// Every template below is both what the writer copies into the buffer and what the field widths are
/// computed from, so editing one moves [`CHUNK_RAW`] with it and the `const` assertions underneath
/// turn "the documentation drifted" into a compile error. That derivation is the whole reason these
/// are byte slices rather than a `format!` call site: a format string cannot be measured.
const AUDIO_PREFIX: &[u8] = b"AUDIO blk=";
/// ` n=` — chunk-count separator, shared by both record kinds so the two cannot disagree about it.
const SEP_N: &[u8] = b" n=";
/// ` c=` — chunk-index separator.
const SEP_C: &[u8] = b" c=";
/// ` d=` — payload separator; everything after it is base64.
const SEP_D: &[u8] = b" d=";
/// `AUDEND blk=` — opens a block-summary body.
const AUDEND_PREFIX: &[u8] = b"AUDEND blk=";
/// ` bytes=` — total-raw-bytes separator.
const SEP_BYTES: &[u8] = b" bytes=";
/// ` crc16=` — block-checksum separator.
const SEP_CRC16: &[u8] = b" crc16=";

/// Digits in `blk`: 65,536 ids before the field wraps.
const BLK_HEX_DIGITS: usize = 4;
/// Digits in `n` and `c`.
const COUNT_HEX_DIGITS: usize = 2;
/// Digits in `crc16` — the same width as the frame trailer, so the two checksums look alike on the
/// wire and one host regex reads both.
const CRC_HEX_DIGITS: usize = 4;

/// Decimal digits needed for `value`, so a numeric field can be sized from the largest number the
/// grammar can produce instead of from a guess.
const fn decimal_width(value: u32) -> usize {
    let mut digits = 1usize;
    let mut remaining = value / 10;
    while remaining > 0 {
        digits += 1;
        remaining /= 10;
    }
    digits
}

/// Fixed length of an `AUDIO` body's header: `AUDIO blk=` + 4 hex + ` n=` + 2 hex + ` c=` + 2 hex +
/// ` d=`. Constant because every field is fixed-width, which is what lets the payload budget be a
/// compile-time subtraction.
pub const AUDIO_HEADER_LEN: usize = AUDIO_PREFIX.len()
    + BLK_HEX_DIGITS
    + SEP_N.len()
    + COUNT_HEX_DIGITS
    + SEP_C.len()
    + COUNT_HEX_DIGITS
    + SEP_D.len();

/// Base64 characters left in the body once the header has taken its share, rounded down to a whole
/// group — a partial group would encode nothing and waste the wire.
const FULL_B64_CHARS: usize = (MAX_BODY - AUDIO_HEADER_LEN) / 4 * 4;

/// Raw bytes carried by a full-size `AUDIO` chunk: 129.
pub const CHUNK_RAW: usize = FULL_B64_CHARS / 4 * 3;

/// Values the 2-digit `n` field cycles through.
const COUNT_FIELD_MODULUS: usize = 1 << (COUNT_HEX_DIGITS * 4);

/// Most chunks one block may hold: 255 — one less than the field's modulus, because 256 has no
/// two-digit representation and emitting it would write `00`, handing the assembler a block that
/// claims no chunks at all. Where TASK-038.02's notes say 256 they mean this modulus; the capacity
/// is what a writer must stay inside.
pub const MAX_CHUNKS_PER_BLOCK: usize = COUNT_FIELD_MODULUS - 1;

/// Largest raw payload one block can describe, hence the widest value `bytes` can honestly carry.
pub const MAX_BLOCK_BYTES: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// Decimal digits in `bytes`: exactly enough for [`MAX_BLOCK_BYTES`], so widening the chunk geometry
/// widens the field with it.
const BYTES_DEC_DIGITS: usize = decimal_width(MAX_BLOCK_BYTES as u32);

/// Ids the `blk` field cycles through before wrapping.
const BLOCK_ID_MODULUS: u32 = 1 << (BLK_HEX_DIGITS * 4);

/// Body length of a full-size `AUDIO` record: 199.
pub const FULL_AUDIO_BODY_LEN: usize = AUDIO_HEADER_LEN + FULL_B64_CHARS;

/// Wire length of a full-size `AUDIO` record including framing: 227.
pub const FULL_AUDIO_FRAME_LEN: usize = PREFIX_LEN + FULL_AUDIO_BODY_LEN + TRAILER_LEN;

/// Longest `AUDEND` body, counting each numeric field at full width.
pub const MAX_AUDEND_BODY_LEN: usize = AUDEND_PREFIX.len()
    + BLK_HEX_DIGITS
    + SEP_N.len()
    + COUNT_HEX_DIGITS
    + SEP_BYTES.len()
    + BYTES_DEC_DIGITS
    + SEP_CRC16.len()
    + CRC_HEX_DIGITS;

// The pins. Each one restates a figure this module's documentation and TASK-038.06 publish; if a
// template or a width changes shape, the build stops here rather than the paper going stale.
const _: () = assert!(AUDIO_HEADER_LEN == 27);
const _: () = assert!(FULL_B64_CHARS == 172);
const _: () = assert!(CHUNK_RAW == 129);
const _: () = assert!(MAX_CHUNKS_PER_BLOCK == 255);
const _: () = assert!(BYTES_DEC_DIGITS == 5);
const _: () = assert!(FULL_AUDIO_BODY_LEN == 199);
const _: () = assert!(FULL_AUDIO_BODY_LEN == MAX_BODY - 1);
const _: () = assert!(FULL_AUDIO_FRAME_LEN == 227);
const _: () = assert!(MAX_AUDEND_BODY_LEN == 43);
const _: () = assert!(MAX_AUDEND_BODY_LEN <= MAX_BODY);

// Widths checked against the values they must carry, on the same compile-time footing as `frame`'s
// own fields (see `frame::check_hex_width` for why the check cannot live in the renderers: those
// run inside the record lock, where a panic masks interrupts permanently). Deleting the runtime
// asserts from `write_hex` / `write_decimal` would otherwise have quietly taken `dump`'s protection
// with them.
const _: () = {
    check_hex_width(BLK_HEX_DIGITS);
    check_hex_width(COUNT_HEX_DIGITS);
    check_hex_width(CRC_HEX_DIGITS);
    check_decimal_fits(MAX_BLOCK_BYTES as u32, BYTES_DEC_DIGITS);
};

// ---------------------------------------------------------------------------
// Pipe headroom policy
// ---------------------------------------------------------------------------

/// Bytes at the tail of the log pipe that no dump commit may take: one maximum-size
/// record, so a `log::info!` or a `STATUS` record can always be committed.
///
/// A dump is bulk storage being shipped after the fact; a log record is the instrument's
/// only live explanation of what it is doing. Letting the former fill the ring would make
/// every drop counter read during a dump measure the traffic the dump crowded out, which
/// is the opposite of evidence. The reserve is therefore a property of the protocol, not a
/// tunable: there is deliberately no capacity argument anywhere in this module, because a
/// parameter here would be a decision the caller did not make.
///
/// [`RESERVE`] is [`MAX_FRAME`] rather than the 227 bytes an `AUDIO` frame needs, so the
/// reservation holds for *any* record the codec can produce, including a full-body `AUDEND`
/// or a log line the writer never anticipated.
pub const RESERVE: usize = MAX_FRAME;

/// Whether committing a dump body of `body_len` bytes leaves at least [`RESERVE`] free.
///
/// Pure and `no_std`: one comparison over values the caller already has, so the whole
/// headroom rule is decidable on the host while the code that acts on it stays on the
/// device. `body_len` is the *body*, not the frame — the prefix and trailer are this
/// function's business, which is what keeps a caller from reserving for one and comparing
/// against the other.
///
/// # Post-condition
///
/// After any commit this admitted, at least [`RESERVE`] bytes remain free in the pipe —
/// stated as a post-condition rather than a probability, and checked exhaustively against
/// a real `embassy_sync` ring in `tests/console_dump.rs`.
///
/// `saturating_sub` keeps the comparison total where `free_capacity < RESERVE`: the ring is
/// inside its own reserve, so nothing may be committed and the answer is `false`, computed
/// without underflow. It is not a clamp hiding an interesting case — the sub-`RESERVE`
/// region is exactly where a dump must stop, and
/// `tests/console_dump.rs::a_dump_can_never_take_the_last_max_frame` sweeps it.
///
/// A `body_len` above [`MAX_BODY`] describes a record the codec cannot build, and yields
/// `false` for every capacity the ring can report; the device's one dump commit path,
/// `try_emit_dump`, refuses such a body outright instead of shipping a shortened chunk.
pub const fn dump_fits(body_len: usize, free_capacity: usize) -> bool {
    let frame_len = PREFIX_LEN + body_len + TRAILER_LEN;
    frame_len <= free_capacity.saturating_sub(RESERVE)
}

// Geometry the headroom rule depends on, pinned rather than assumed: the reserve is one
// maximum frame, so a full dump frame must be strictly smaller than it or the reservation
// would admit frames it cannot pay for.
const _: () = assert!(FULL_AUDIO_FRAME_LEN < RESERVE);
const _: () = assert!(RESERVE == PREFIX_LEN + MAX_BODY + TRAILER_LEN);

/// Why a record body could not be written. Every variant is a refusal: nothing here shortens a
/// payload to make it fit, because a chunk that silently lost its tail reassembles into audio that
/// sounds fine and measures wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyError {
    /// `chunks` exceeds [`MAX_CHUNKS_PER_BLOCK`]; the `n` field cannot name that many.
    TooManyChunks { chunks: usize },
    /// `chunk_index` is not below `chunks`, which includes `chunks == 0` — a block with no chunks has
    /// no chunk zero to send.
    ChunkIndexOutOfRange { chunk_index: usize, chunks: usize },
    /// A chunk with no bytes carries no information and would decode as an empty string, which the
    /// grammar reserves for nothing.
    EmptyChunk,
    /// `raw` needs more than the 172 characters the payload field owns, i.e. more than
    /// [`CHUNK_RAW`] bytes.
    ChunkTooLong { len: usize },
    /// `total_bytes` exceeds [`MAX_BLOCK_BYTES`], so `bytes` has no representation for it.
    BlockTooLarge { total_bytes: usize },
}

/// Append `template` at `at` in `dst`, returning the offset past it.
fn put(dst: &mut [u8], at: usize, template: &[u8]) -> usize {
    dst[at..at + template.len()].copy_from_slice(template);
    at + template.len()
}

/// Append `value` as `digits` lowercase hex digits at `at`, returning the offset past them.
fn put_hex(dst: &mut [u8], at: usize, value: u32, digits: usize) -> usize {
    write_hex(value, &mut dst[at..at + digits]);
    at + digits
}

/// Append `value` as `digits` decimal digits at `at`, returning the offset past them.
fn put_decimal(dst: &mut [u8], at: usize, value: u32, digits: usize) -> usize {
    write_decimal(value, &mut dst[at..at + digits]);
    at + digits
}

/// Write an `AUDIO blk=… n=… c=… d=…` body into `out`, returning its length.
///
/// One chunk of one block. `chunks` is how many chunks the block will have in total, `chunk_index`
/// this chunk's zero-based position, and `block_index` the block's id — masked to the field's
/// modulus, so the wrap at 65,536 blocks is a documented property of the wire rather than a
/// side effect of digit truncation.
///
/// Refusals come from [`BodyError`]; on any of them `out` is untouched.
pub fn audio_body(
    block_index: u32,
    chunks: u16,
    chunk_index: u16,
    raw: &[u8],
    out: &mut [u8; MAX_BODY],
) -> Result<usize, BodyError> {
    let chunks = chunks as usize;
    let chunk_index = chunk_index as usize;
    if chunks > MAX_CHUNKS_PER_BLOCK {
        return Err(BodyError::TooManyChunks { chunks });
    }
    if chunk_index >= chunks {
        return Err(BodyError::ChunkIndexOutOfRange {
            chunk_index,
            chunks,
        });
    }
    if raw.is_empty() {
        return Err(BodyError::EmptyChunk);
    }
    // Checked before a single byte is written, so a refusal never leaves a half-built body behind:
    // deferring the budget check to `encode` would mean the header had already landed in the
    // caller's buffer by the time the payload was refused.
    let chars = encoded_len(raw.len());
    if chars > FULL_B64_CHARS {
        return Err(BodyError::ChunkTooLong { len: raw.len() });
    }

    let dst = &mut out[..];
    let mut at = 0usize;
    at = put(dst, at, AUDIO_PREFIX);
    at = put_hex(dst, at, block_index % BLOCK_ID_MODULUS, BLK_HEX_DIGITS);
    at = put(dst, at, SEP_N);
    at = put_hex(dst, at, chunks as u32, COUNT_HEX_DIGITS);
    at = put(dst, at, SEP_C);
    at = put_hex(dst, at, chunk_index as u32, COUNT_HEX_DIGITS);
    at = put(dst, at, SEP_D);

    // The budget check above means this cannot fail; mapping rather than unwrapping keeps the
    // refusal path free of panics regardless.
    let written = encode(raw, &mut dst[at..at + chars])
        .map_err(|_| BodyError::ChunkTooLong { len: raw.len() })?;
    debug_assert_eq!(written, chars);
    Ok(at + written)
}

/// Write an `AUDEND blk=… n=… bytes=… crc16=…` body into `out`, returning its length.
///
/// The block summary that closes a block: `total_bytes` is the raw byte count the chunks added up to
/// and `crc` is [`crate::frame::crc16_ccitt`] over those raw concatenated bytes, both computed by the
/// caller that assembled the block. Nothing here touches either — the device computes the checksum as
/// it fills a ring, and the host recomputes it from what it received, so this function's job ends at
/// formatting.
pub fn audend_body(
    block_index: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
    out: &mut [u8; MAX_BODY],
) -> Result<usize, BodyError> {
    let chunks = chunks as usize;
    if chunks > MAX_CHUNKS_PER_BLOCK {
        return Err(BodyError::TooManyChunks { chunks });
    }
    if total_bytes as usize > MAX_BLOCK_BYTES {
        return Err(BodyError::BlockTooLarge {
            total_bytes: total_bytes as usize,
        });
    }

    let dst = &mut out[..];
    let mut at = 0usize;
    at = put(dst, at, AUDEND_PREFIX);
    at = put_hex(dst, at, block_index % BLOCK_ID_MODULUS, BLK_HEX_DIGITS);
    at = put(dst, at, SEP_N);
    at = put_hex(dst, at, chunks as u32, COUNT_HEX_DIGITS);
    at = put(dst, at, SEP_BYTES);
    at = put_decimal(dst, at, total_bytes, BYTES_DEC_DIGITS);
    at = put(dst, at, SEP_CRC16);
    at = put_hex(dst, at, u32::from(crc), CRC_HEX_DIGITS);
    Ok(at)
}

/// Build and frame a complete `AUDIO` record, returning what landed in `frame`.
///
/// The composite callers should reach for: it does the body and [`frame::encode`] in one step, so no
/// caller re-derives the two-buffer dance. Both buffers are explicit rather than a hidden static
/// because the device supplies `frame` from the existing `RECORD_BUFS` (TASK-030.02's lock-guarded
/// set) and pays nothing on the stack for it.
///
/// `level` is the caller's, and [`Level::Info`] is what BOOT and STATUS use: these are
/// device-generated facts about a capture, not diagnostics, and the Debug filter that quiets chatter
/// should not quietly silence a measurement. The decoder accepts all five letters, so a caller that
/// wants dumps filtered with verbose logging may pass [`Level::Debug`] instead.
#[allow(clippy::too_many_arguments)] // Nine inputs, seven of which are the record: the two records
                                     // describe one block and get read side by side, so folding fields
                                     // into a struct would hide `blk`/`n`/`c` behind a name without
                                     // shortening what a caller must actually supply.
pub fn audio_record(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    chunk_index: u16,
    raw: &[u8],
    body: &mut [u8; MAX_BODY],
    frame_buf: &mut [u8; MAX_FRAME],
) -> Result<Encoded, BodyError> {
    let len = audio_body(block_index, chunks, chunk_index, raw, body)?;
    Ok(frame::encode(level, seq, now_ms, &body[..len], frame_buf))
}

/// Build and frame a complete `AUDEND` record, returning what landed in `frame`.
///
/// The composite counterpart of [`audio_record`], with the same reasoning about buffers and level.
#[allow(clippy::too_many_arguments)] // Same reasoning as [`audio_record`]; the parameter lists mirror
                                     // each other field for field on purpose.
pub fn audend_record(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
    body: &mut [u8; MAX_BODY],
    frame_buf: &mut [u8; MAX_FRAME],
) -> Result<Encoded, BodyError> {
    let len = audend_body(block_index, chunks, total_bytes, crc, body)?;
    Ok(frame::encode(level, seq, now_ms, &body[..len], frame_buf))
}

// ---------------------------------------------------------------------------
// Host-side block assembly — refusing a block that cannot be proved complete
// ---------------------------------------------------------------------------

/// Bits per presence-mask word.
const MASK_BITS: usize = 64;

/// Words needed for one bit per chunk index the `n` field can name. The modulus is used rather
/// than [`MAX_CHUNKS_PER_BLOCK`] so index 255 lands inside a word without a bounds check anywhere
/// else, and so widening the field widens the mask with it.
const MASK_WORDS: usize = COUNT_FIELD_MODULUS.div_ceil(MASK_BITS);

/// Indices a printed missing-list names before ellipsising. A block missing all 255 chunks is one
/// lost span of wire, not 255 facts worth scrolling past.
const DEBUG_MISSING_INDICES: usize = 12;

/// Which chunks a refused block never received.
///
/// A refusal is only actionable if it names what is absent: “block 3 failed” tells an operator
/// nothing, “block 3 is missing chunks 4 and 5 of 6” says a specific run of records went away. The
/// set is carried inline because `no_std` has no `Vec` to hand out, and 32 bytes costs less than
/// the reasoning a heap allocation would need.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MissingChunks {
    /// Bit `i` set ⇒ chunk index `i` never arrived.
    words: [u64; MASK_WORDS],
    count: usize,
}

impl MissingChunks {
    /// Nothing missing — the mask a caller starts from before recording anything absent.
    const NONE: Self = Self {
        words: [0; MASK_WORDS],
        count: 0,
    };

    fn insert(&mut self, index: u16) {
        self.words[(index as usize) / MASK_BITS] |= 1 << ((index as usize) % MASK_BITS);
        self.count += 1;
    }

    /// How many chunks the refused block never received.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Whether chunk `index` is among those never received. An index the grammar cannot name is
    /// simply not missing, which keeps this total for any `u16` instead of panicking on one.
    pub fn contains(&self, index: u16) -> bool {
        let slot = index as usize;
        slot < COUNT_FIELD_MODULUS && (self.words[slot / MASK_BITS] >> (slot % MASK_BITS)) & 1 == 1
    }

    /// Lowest missing index — the one a one-line diagnostic should name.
    pub fn first(&self) -> Option<u16> {
        self.after(0)
    }

    /// Lowest missing index at or after `from`. Linear over a field the grammar caps at 256 values:
    /// bit-scanning arithmetic here would be less code nobody reads, not measurably more speed.
    fn after(&self, from: u16) -> Option<u16> {
        (from..COUNT_FIELD_MODULUS as u16).find(|index| self.contains(*index))
    }
}

/// Print the indices themselves: these values land in test failures and capture summaries, where
/// twenty-five lines of bitfield internals describe nothing.
impl core::fmt::Debug for MissingChunks {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} missing [", self.count)?;
        let mut shown = 0usize;
        let mut separator = "";
        let mut cursor = 0u16;
        while shown < DEBUG_MISSING_INDICES {
            let Some(index) = self.after(cursor) else {
                break;
            };
            write!(f, "{separator}{index}")?;
            separator = " ";
            shown += 1;
            cursor = index + 1;
        }
        if shown < self.count {
            write!(f, "{separator}…")?;
        }
        write!(f, "]")
    }
}

/// One thing a consumed record did.
///
/// Every variant names its block, so a caller logging events never reconstructs which block an
/// event belonged to. There is deliberately no `Started` variant: a block announces itself by its
/// first stored chunk, and an event whose only content is “state changed” asks the caller to infer
/// the consequence from context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A chunk landed at `chunk · CHUNK_RAW` for the first time.
    ChunkStored {
        block: u32,
        chunk: u16,
        bytes: usize,
    },
    /// The same chunk index arrived again carrying byte-identical data: harmless, and expected
    /// whenever a writer retries a record it could not confirm had left the device.
    Duplicate { block: u32, chunk: u16 },
    /// The same chunk index arrived carrying *different* bytes. The earlier copy is kept — a
    /// re-send arriving after a partial write must not overwrite data that already validated
    /// against the block's own checksum — and the block is marked conflicted.
    Conflict { block: u32, chunk: u16 },
    /// Every chunk arrived, the byte counts agree with the geometry, and the block checksum matches
    /// the assembled bytes. [`BlockAssembler::pcm`] names them.
    Complete { block: u32, bytes: usize },
    /// This block will not yield samples. The reason names what to look for on the wire.
    Failed { block: u32, reason: Failure },
    /// A record for a block that has already closed: counted and dropped, because applying it would
    /// mean reopening a verdict this assembler already published.
    LateRecord { block: u32, chunk: Option<u16> },
    /// A record named a different block while one was open, so the open block's remaining chunks
    /// are gone for good. Carries what had not arrived when it died.
    Abandoned { block: u32, missing: MissingChunks },
    /// A body opened with a dump verb but does not have that verb's shape. `at` is the first byte
    /// that broke the grammar, counted from the start of the body.
    Malformed { at: usize },
    /// Not dump traffic at all — `BOOT`, `STATUS`, `PANIC`, a log line. Passing uncounted as loss is
    /// the point: this assembler sits on the receiving end of a console stream, not a dedicated
    /// channel that does not exist.
    Ignored,
}

/// Why a block was refused.
///
/// These are not severities. Each one points at something different to go looking for, and several
/// can only be told apart by the numbers they carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Chunks never arrived. Loss proven by sequence, which is the property no checksum supplies.
    Missing(MissingChunks),
    /// All chunks arrived and [`crc16_ccitt`] over the assembled bytes disagrees with what `AUDEND`
    /// shipped.
    Crc { shipped: u16, computed: u16 },
    /// Chunk `index` was sent twice with different bytes, so the block has two candidate shapes.
    Conflict { index: u16 },
    /// The caller's staging buffer cannot hold a chunk at its geometric offset; the block needs at
    /// least `needed` bytes.
    Capacity { needed: usize },
    /// The block's size does not add up. `placed` is what the chunks actually carried, `expected` is
    /// what the geometry requires — `(n − 1) · CHUNK_RAW` plus the final chunk — and `declared` is
    /// what `AUDEND` claimed. Comparing placed against expected is what proves every non-final chunk
    /// was full: placement puts chunk `i` at `i · CHUNK_RAW`, so a short chunk anywhere but last
    /// leaves a hole no honest total can account for.
    Length {
        declared: u32,
        placed: usize,
        expected: usize,
    },
    /// Chunks of one block disagree about how many chunks the block has, so its geometry has no
    /// single answer. Only corruption produces this: one writer emits one `n` per block.
    ChunkCount { expected: u16, found: u16 },
}

/// The one or two events a single consumed record produced, oldest first.
///
/// Two happen when a record names a block other than the one open: the open block loses its last
/// chance to complete *and* the arriving record gets its own verdict. Both are reported rather than
/// the more interesting one being chosen, because a tool whose entire job is proving absence may not
/// drop the fact that a block died. At most two can occur: a record names one block, so it can kill
/// at most one other block and decide at most itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Actions {
    /// Events in order. Slots past [`Actions::len`] hold [`Action::Ignored`] purely to keep the
    /// array initialised; `as_slice` never shows them.
    items: [Action; 2],
    len: usize,
}

impl Actions {
    /// Empty — the state `accept` starts from before the record has been classified.
    fn none() -> Self {
        Self {
            items: [Action::Ignored; 2],
            len: 0,
        }
    }

    fn push(&mut self, action: Action) {
        // Two is the provable maximum (see the type documentation); a third would mean the state
        // machine started reporting things it did not do, so debug builds stop rather than choose.
        debug_assert!(
            self.len < self.items.len(),
            "more than two events per record"
        );
        if self.len < self.items.len() {
            self.items[self.len] = action;
            self.len += 1;
        }
    }

    /// The events in order. Never empty in practice: every body is either a dump verb or
    /// [`Action::Ignored`].
    pub fn as_slice(&self) -> &[Action] {
        &self.items[..self.len]
    }

    /// The verdict on the record just consumed — the last event when a record also killed an older
    /// block, which is the overwhelmingly common single-event case too.
    pub fn last(&self) -> Option<Action> {
        self.as_slice().last().copied()
    }
}

/// Print the events as the list they are, not as an array padded with placeholders.
impl core::fmt::Debug for Actions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

/// What a [`BlockAssembler`] saw, in the categories a capture summary reports.
///
/// The assembler counts these itself rather than leaving callers to tally returned [`Action`]s: the
/// counting then has one owner, and a manifest cannot drift from the state machine that produced it.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Records consumed, dump traffic and log chatter alike.
    pub records: u64,
    /// Records that were neither `AUDIO` nor `AUDEND` — the console stream flowing past untouched.
    pub non_dump_records: u64,
    /// Dump-shaped bodies that did not parse. Never charged as loss on their own: a chunk that never
    /// got stored surfaces again as [`Failure::Missing`], which is the honest accounting.
    pub malformed_records: u64,
    pub chunks_stored: u64,
    pub duplicate_chunks: u64,
    pub conflicts: u64,
    pub late_records: u64,
    pub blocks_completed: u64,
    pub blocks_failed: u64,
    pub blocks_abandoned: u64,
    /// Raw PCM bytes handed out by completed blocks.
    pub pcm_bytes: u64,
}

impl Tally {
    /// Charge one event. One funnel, so a new variant cannot escape the counters by forgetting a
    /// call site.
    fn observe(&mut self, action: &Action) {
        match action {
            Action::ChunkStored { .. } => self.chunks_stored += 1,
            Action::Duplicate { .. } => self.duplicate_chunks += 1,
            Action::Conflict { .. } => self.conflicts += 1,
            Action::Complete { bytes, .. } => {
                self.blocks_completed += 1;
                self.pcm_bytes += *bytes as u64;
            }
            Action::Failed { .. } => self.blocks_failed += 1,
            Action::LateRecord { .. } => self.late_records += 1,
            Action::Abandoned { .. } => self.blocks_abandoned += 1,
            Action::Malformed { .. } => self.malformed_records += 1,
            Action::Ignored => self.non_dump_records += 1,
        }
    }
}

/// A block that died before its summary arrived, and what it still needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abandoned {
    pub block: u32,
    pub missing: MissingChunks,
}

/// What [`BlockAssembler::finish`] reported about a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finish {
    /// The block still open when the bytes ran out. Its remaining chunks may merely not have been
    /// sent yet, but nothing further can arrive, so it is indistinguishable from loss and refused as
    /// such.
    pub abandoned: Option<Abandoned>,
    pub tally: Tally,
}

/// A block whose chunks are arriving but whose summary has not.
#[derive(Clone, Copy)]
struct OpenBlock {
    block: u32,
    /// Chunk count as announced by this block's own chunks, not by a summary.
    chunks: u16,
    present: [u64; MASK_WORDS],
    /// Distinct indices stored.
    stored: usize,
    /// Raw bytes those indices carried, summed once per index.
    placed: usize,
    /// Raw length of the chunk at index `chunks - 1`; zero until it arrives.
    final_len: usize,
    /// First index seen twice with different bytes.
    conflict: Option<u16>,
}

impl OpenBlock {
    fn new(block: u32, chunks: u16) -> Self {
        Self {
            block,
            chunks,
            present: [0; MASK_WORDS],
            stored: 0,
            placed: 0,
            final_len: 0,
            conflict: None,
        }
    }

    fn is_present(&self, index: u16) -> bool {
        (self.present[(index as usize) / MASK_BITS] >> ((index as usize) % MASK_BITS)) & 1 == 1
    }

    fn mark_present(&mut self, index: u16) {
        self.present[(index as usize) / MASK_BITS] |= 1 << ((index as usize) % MASK_BITS);
        self.stored += 1;
    }

    fn complete(&self) -> bool {
        self.stored == self.chunks as usize
    }

    fn missing(&self) -> MissingChunks {
        let mut missing = MissingChunks::NONE;
        for index in 0..self.chunks {
            if !self.is_present(index) {
                missing.insert(index);
            }
        }
        missing
    }
}

/// Where each `AUDIO` field starts. Derived from the writer's own templates, so editing a template
/// moves the parser and its diagnostics with it, and the assertion below turns drift into a build
/// failure rather than a parser quietly reading the wrong bytes.
const CHUNK_BLK_AT: usize = AUDIO_PREFIX.len();
const CHUNK_COUNT_AT: usize = CHUNK_BLK_AT + BLK_HEX_DIGITS + SEP_N.len();
const CHUNK_INDEX_AT: usize = CHUNK_COUNT_AT + COUNT_HEX_DIGITS + SEP_C.len();
const CHUNK_PAYLOAD_AT: usize = CHUNK_INDEX_AT + COUNT_HEX_DIGITS + SEP_D.len();

/// Where each `AUDEND` field starts.
const SUMMARY_BLK_AT: usize = AUDEND_PREFIX.len();
const SUMMARY_COUNT_AT: usize = SUMMARY_BLK_AT + BLK_HEX_DIGITS + SEP_N.len();
const SUMMARY_BYTES_AT: usize = SUMMARY_COUNT_AT + COUNT_HEX_DIGITS + SEP_BYTES.len();
const SUMMARY_CRC_AT: usize = SUMMARY_BYTES_AT + BYTES_DEC_DIGITS + SEP_CRC16.len();

const _: () = assert!(CHUNK_PAYLOAD_AT == AUDIO_HEADER_LEN);
const _: () = assert!(SUMMARY_CRC_AT + CRC_HEX_DIGITS == MAX_AUDEND_BODY_LEN);

/// Positional reader over a body's fixed-width skeleton.
///
/// On failure `at` stays where it was, which is what lets [`Action::Malformed`] point at a byte
/// instead of merely reporting that something was wrong.
struct BodyReader<'b> {
    body: &'b [u8],
    at: usize,
}

impl<'b> BodyReader<'b> {
    fn new(body: &'b [u8]) -> Self {
        Self { body, at: 0 }
    }

    /// Consume `template`, or fail without moving.
    fn literal(&mut self, template: &[u8]) -> Option<()> {
        let end = self.at.checked_add(template.len())?;
        if self.body.get(self.at..end) == Some(template) {
            self.at = end;
            Some(())
        } else {
            None
        }
    }

    /// Consume exactly `digits` lowercase hex digits. Uppercase is not in the grammar, so the frame
    /// parser's rule is reused rather than restated: two spellings of “valid” is one too many.
    fn hex(&mut self, digits: usize) -> Option<u32> {
        let end = self.at.checked_add(digits)?;
        let value = parse_hex(self.body.get(self.at..end)?)?;
        self.at = end;
        Some(value)
    }

    fn decimal(&mut self, digits: usize) -> Option<u32> {
        let end = self.at.checked_add(digits)?;
        let value = parse_decimal(self.body.get(self.at..end)?)?;
        self.at = end;
        Some(value)
    }

    /// Everything from here to the end of the body. Reached only after successful steps, so `at` is
    /// inside the body by construction.
    fn rest(&self) -> &'b [u8] {
        &self.body[self.at..]
    }
}

/// The fields of an `AUDIO` body, payload left encoded: only the assembler decodes it, and only into
/// a block's slot.
struct ChunkFields<'b> {
    block: u32,
    chunks: u16,
    index: u16,
    payload: &'b [u8],
}

/// The fields of an `AUDEND` body.
struct SummaryFields {
    block: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
}

/// Parse an `AUDIO` body; on failure, the body offset that broke the shape.
fn parse_chunk(body: &[u8]) -> Result<ChunkFields<'_>, usize> {
    let mut reader = BodyReader::new(body);
    reader.literal(AUDIO_PREFIX).ok_or(reader.at)?;
    let block = reader.hex(BLK_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_N).ok_or(reader.at)?;
    let chunks = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_C).ok_or(reader.at)?;
    let index = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_D).ok_or(reader.at)?;
    Ok(ChunkFields {
        block,
        // Both fields are two hex digits wide, so the values cannot exceed 255 and truncating to
        // `u16` drops nothing.
        chunks: chunks as u16,
        index: index as u16,
        payload: reader.rest(),
    })
}

/// Parse an `AUDEND` body; on failure, the body offset that broke the shape.
fn parse_summary(body: &[u8]) -> Result<SummaryFields, usize> {
    let mut reader = BodyReader::new(body);
    reader.literal(AUDEND_PREFIX).ok_or(reader.at)?;
    let block = reader.hex(BLK_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_N).ok_or(reader.at)?;
    let chunks = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_BYTES).ok_or(reader.at)?;
    let total_bytes = reader.decimal(BYTES_DEC_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_CRC16).ok_or(reader.at)?;
    let crc = reader.hex(CRC_HEX_DIGITS).ok_or(reader.at)?;
    Ok(SummaryFields {
        block,
        chunks: chunks as u16,
        total_bytes,
        crc: crc as u16,
    })
}

/// Assemble `AUDIO`/`AUDEND` records into whole blocks of raw PCM, refusing every block whose
/// completeness it cannot prove.
///
/// Feed it [`crate::frame::Record::body`] slices as the console decoder delivers them: it parses the
/// dump verbs itself and ignores everything else. All it needs from the caller is one staging buffer,
/// which it borrows for as long as it lives:
///
/// ```text
/// let mut staging = [0u8; 8192];
/// let mut assembler = BlockAssembler::new(&mut staging);
/// while let Some(record) = decoder.next_record() {
///     for action in assembler.accept(record.body).as_slice() {
///         if let Action::Complete { bytes, .. } = action {
///             file.write_all(assembler.pcm())?;   // exactly `bytes` long
///         }
///     }
/// }
/// let finish = assembler.finish();
/// ```
///
/// # Completeness is proved by sequence, never by checksum alone
///
/// A block completes when, and only when:
///
/// 1. every chunk index `0..n` arrived at least once,
/// 2. no index arrived twice with different bytes,
/// 3. the bytes the chunks carry add up both to what the geometry requires — `(n − 1) · CHUNK_RAW`
///    plus the final chunk — and to what `AUDEND` declared, and
/// 4. [`crc16_ccitt`] over the assembled bytes equals the value `AUDEND` shipped.
///
/// Condition 1 is the one no checksum supplies. [`crate::frame`] documents that a record whose
/// leading `~` vanished produces no integrity failure at all, so a lost chunk looks exactly like a
/// quiet wire; `n_of_n` is what turns that silence into a refusal. In the other direction a CRC
/// cannot detect permutation, which is why chunks are *placed* at `chunk_index · CHUNK_RAW` rather
/// than appended: arrival order then cannot change the result structurally rather than by luck, and
/// `tests/console_dump.rs` shuffles streams to show it rather than assume it.
///
/// # One open block, because interleaving is loss rather than concurrency
///
/// A single producer writes a block's chunks in order, so a record naming a different block while
/// one is open means the open block's tail is gone. That block is reported [`Action::Abandoned`]
/// with its missing list and the new one takes its place. Holding several blocks open would require
/// deciding which gaps were real, and the answer would be a heuristic sitting underneath a tool whose
/// whole value is certainty.
///
/// Only the most recently closed block id is remembered, which is enough to classify the realistic
/// late arrival: a re-send of the block that just finished. A chunk from one further back opens a
/// fresh block and refuses whatever it displaced — still a refusal, never a false completion.
pub struct BlockAssembler<'a> {
    /// Assembled bytes for the block in play, indexed by chunk geometry.
    buf: &'a mut [u8],
    open: Option<OpenBlock>,
    /// Most recent block that reached a verdict, so a record arriving after it is recognised as late
    /// instead of resurrecting a published decision.
    last_closed: Option<u32>,
    tally: Tally,
    /// Length of the last completed block, marking the meaningful prefix of `buf`. Zero whenever a
    /// block other than a completed one is in play, so a caller cannot read out another block's
    /// leftovers by holding on to a stale view.
    completed: usize,
}

impl<'a> BlockAssembler<'a> {
    /// An assembler writing into `buf`, which must outlive the assembler.
    ///
    /// No minimum size is imposed: a buffer too small for a block is discovered when a chunk cannot
    /// be placed and reported as [`Failure::Capacity`], which beats a constructor that refuses to
    /// build until it knows the block geometry it has not seen yet.
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            open: None,
            last_closed: None,
            tally: Tally::default(),
            completed: 0,
        }
    }

    /// Consume one validated record's body and say what it did.
    pub fn accept(&mut self, body: &[u8]) -> Actions {
        self.tally.records += 1;
        let mut actions = Actions::none();
        if body.starts_with(AUDIO_PREFIX) {
            self.consume_chunk(body, &mut actions);
        } else if body.starts_with(AUDEND_PREFIX) {
            self.consume_summary(body, &mut actions);
        } else {
            self.report(Action::Ignored, &mut actions);
        }
        actions
    }

    /// End the stream: report the block that was still open and hand over the counters.
    pub fn finish(self) -> Finish {
        let mut tally = self.tally;
        let abandoned = match self.open {
            Some(open) => {
                // Charged here rather than through `Tally::observe`, which takes a reference to an
                // event this assembler no longer needs to describe to anyone but the counters.
                tally.blocks_abandoned += 1;
                Some(Abandoned {
                    block: open.block,
                    missing: open.missing(),
                })
            }
            None => None,
        };
        Finish { abandoned, tally }
    }

    /// The most recently completed block's raw bytes: mono 16-bit capture, ready to interleave or
    /// hash unchanged.
    ///
    /// Meaningful between an [`Action::Complete`] and the moment another block starts, and empty
    /// otherwise. Handing these out through the assembler rather than on the action keeps the event
    /// `Copy` and avoids a second mutable view of the same buffer.
    pub fn pcm(&self) -> &[u8] {
        &self.buf[..self.completed]
    }

    /// Charge an event and record it for the caller, so no path can update one of the two views and
    /// forget the other.
    fn report(&mut self, action: Action, out: &mut Actions) {
        self.tally.observe(&action);
        out.push(action);
    }

    /// Begin a block, invalidating any bytes a previous completion left claimable.
    fn begin(&mut self, block: u32, chunks: u16) -> OpenBlock {
        self.completed = 0;
        OpenBlock::new(block, chunks)
    }

    /// Handle an `AUDIO` body: parse, resolve which block it belongs to, then store or refuse.
    fn consume_chunk(&mut self, body: &[u8], out: &mut Actions) {
        let chunk = match parse_chunk(body) {
            Ok(chunk) => chunk,
            Err(at) => {
                self.report(Action::Malformed { at }, out);
                return;
            }
        };

        // Grammar before geometry: `n = 0` has no chunk zero to receive, and an index at or past `n`
        // claims a place in a block that says it has none.
        if chunk.chunks == 0 {
            self.report(Action::Malformed { at: CHUNK_COUNT_AT }, out);
            return;
        }
        if chunk.index >= chunk.chunks {
            self.report(Action::Malformed { at: CHUNK_INDEX_AT }, out);
            return;
        }

        // Identity resolves before the payload is decoded, so a record belonging to a block that has
        // already closed is called late whether or not its contents would have parsed.
        let prior = self.open.take();
        let mut block = match prior {
            Some(open) if open.block == chunk.block => {
                if open.chunks != chunk.chunks {
                    let block = open.block;
                    self.last_closed = Some(block);
                    self.completed = 0;
                    let reason = Failure::ChunkCount {
                        expected: open.chunks,
                        found: chunk.chunks,
                    };
                    self.report(Action::Failed { block, reason }, out);
                    return;
                }
                open
            }
            Some(open) => {
                let missing = open.missing();
                self.report(
                    Action::Abandoned {
                        block: open.block,
                        missing,
                    },
                    out,
                );
                self.begin(chunk.block, chunk.chunks)
            }
            None => {
                if self.last_closed == Some(chunk.block) {
                    self.report(
                        Action::LateRecord {
                            block: chunk.block,
                            chunk: Some(chunk.index),
                        },
                        out,
                    );
                    return;
                }
                self.begin(chunk.block, chunk.chunks)
            }
        };

        // Strict decoding happens here rather than in the caller: a payload that is not canonical
        // base64 is corruption the frame checksum let through. Such a chunk is not kept and the
        // block is left exactly as it was, because the honest consequence of a chunk nobody can read
        // is that the index is missing — which fails the block at its summary with the missing list
        // naming it. Refusing the block here instead would invent a second way for one to die and
        // lose the diagnosis the missing list carries.
        let mut raw = [0u8; CHUNK_RAW];
        let written = match decode_payload(chunk.payload, &mut raw) {
            Ok(written) => written,
            Err(at) => {
                self.open = Some(block);
                self.report(Action::Malformed { at }, out);
                return;
            }
        };

        let action = place(&mut block, self.buf, chunk.index, &raw[..written]);
        // Only a refusal ends the block: storing, duplicating, and conflicting all leave it waiting
        // for the rest of its chunks.
        if keeps_open(&action) {
            self.open = Some(block);
        } else {
            self.last_closed = Some(block.block);
            self.completed = 0;
        }
        self.report(action, out);
    }

    /// Handle an `AUDEND` body: decide the block it names, or refuse a block no chunk ever reached.
    fn consume_summary(&mut self, body: &[u8], out: &mut Actions) {
        let summary = match parse_summary(body) {
            Ok(summary) => summary,
            Err(at) => {
                self.report(Action::Malformed { at }, out);
                return;
            }
        };
        if summary.chunks == 0 {
            self.report(
                Action::Malformed {
                    at: SUMMARY_COUNT_AT,
                },
                out,
            );
            return;
        }

        let open = match self.open.take() {
            Some(open) if open.block == summary.block => open,
            Some(open) => {
                let missing = open.missing();
                self.report(
                    Action::Abandoned {
                        block: open.block,
                        missing,
                    },
                    out,
                );
                self.refuse_without_chunks(&summary, out);
                return;
            }
            None => {
                if self.last_closed == Some(summary.block) {
                    self.report(
                        Action::LateRecord {
                            block: summary.block,
                            chunk: None,
                        },
                        out,
                    );
                    return;
                }
                self.refuse_without_chunks(&summary, out);
                return;
            }
        };

        let action = verdict(&open, &summary, self.buf);
        self.last_closed = Some(summary.block);
        self.completed = match action {
            Action::Complete { bytes, .. } => bytes,
            _ => 0,
        };
        self.report(action, out);
    }

    /// Refuse a block a summary describes but no chunk ever reached: `n` says how many were owed and
    /// none arrived, so the missing list is the whole block.
    fn refuse_without_chunks(&mut self, summary: &SummaryFields, out: &mut Actions) {
        let mut missing = MissingChunks::NONE;
        for index in 0..summary.chunks {
            missing.insert(index);
        }
        self.last_closed = Some(summary.block);
        self.completed = 0;
        self.report(
            Action::Failed {
                block: summary.block,
                reason: Failure::Missing(missing),
            },
            out,
        );
    }
}

/// Store a decoded chunk in its geometric slot, or explain why the block is over.
///
/// Deliberately free of `self`: the staging buffer and the block's own state get decided together
/// here, and the caller only has to put the block back if it survived.
fn place(block: &mut OpenBlock, buf: &mut [u8], index: u16, raw: &[u8]) -> Action {
    let start = index as usize * CHUNK_RAW;
    let end = start + raw.len();
    let id = block.block;
    if end > buf.len() {
        return Action::Failed {
            block: id,
            reason: Failure::Capacity { needed: end },
        };
    }

    // Duplicates are decided by content, not by having seen the index before: a retransmission of
    // identical bytes is the retry a lossy link expects, while the same index carrying different
    // bytes means two writers, or one writer and a corrupted copy, and no amount of preferring the
    // newer arrival makes either one trustworthy.
    if block.is_present(index) {
        return if buf[start..end] == *raw {
            Action::Duplicate {
                block: id,
                chunk: index,
            }
        } else {
            if block.conflict.is_none() {
                block.conflict = Some(index);
            }
            Action::Conflict {
                block: id,
                chunk: index,
            }
        };
    }

    buf[start..end].copy_from_slice(raw);
    block.mark_present(index);
    block.placed += raw.len();
    if index + 1 == block.chunks {
        block.final_len = raw.len();
    }
    Action::ChunkStored {
        block: id,
        chunk: index,
        bytes: raw.len(),
    }
}

/// Whether a chunk-path event leaves the block waiting for more chunks.
fn keeps_open(action: &Action) -> bool {
    matches!(
        action,
        Action::ChunkStored { .. } | Action::Duplicate { .. } | Action::Conflict { .. }
    )
}

/// Decide a block whose summary has arrived.
///
/// Ordered by how much each refusal explains: a conflict or a missing list says where to look, a
/// checksum mismatch only says the bytes are not the ones that were sent.
fn verdict(open: &OpenBlock, summary: &SummaryFields, pcm: &[u8]) -> Action {
    let id = open.block;
    if open.chunks != summary.chunks {
        return Action::Failed {
            block: id,
            reason: Failure::ChunkCount {
                expected: open.chunks,
                found: summary.chunks,
            },
        };
    }
    if let Some(index) = open.conflict {
        return Action::Failed {
            block: id,
            reason: Failure::Conflict { index },
        };
    }
    if !open.complete() {
        return Action::Failed {
            block: id,
            reason: Failure::Missing(open.missing()),
        };
    }
    let expected = (open.chunks as usize - 1) * CHUNK_RAW + open.final_len;
    if open.placed != expected || open.placed != summary.total_bytes as usize {
        return Action::Failed {
            block: id,
            reason: Failure::Length {
                declared: summary.total_bytes,
                placed: open.placed,
                expected,
            },
        };
    }
    let computed = crc16_ccitt(&pcm[..open.placed]);
    if computed != summary.crc {
        return Action::Failed {
            block: id,
            reason: Failure::Crc {
                shipped: summary.crc,
                computed,
            },
        };
    }
    Action::Complete {
        block: id,
        bytes: open.placed,
    }
}

/// Strictly decode a chunk payload into `raw`; on refusal, the body offset to blame.
///
/// Room is checked against the geometry before decoding, so a body longer than the grammar allows is
/// refused as malformed rather than as an undersized output buffer — the buffer is sized by
/// [`CHUNK_RAW`], which is the same fact stated twice.
fn decode_payload(payload: &[u8], raw: &mut [u8]) -> Result<usize, usize> {
    if max_raw_for(payload.len()) > CHUNK_RAW {
        return Err(CHUNK_PAYLOAD_AT);
    }
    match decode(payload, raw) {
        // A payload with no bytes carries no information and no writer emits one.
        Ok(0) | Err(DecodeError::OutputTooSmall { .. }) => Err(CHUNK_PAYLOAD_AT),
        Ok(written) => Ok(written),
        Err(error) => Err(payload_offset(error_offset(&error))),
    }
}

/// Body offset a [`DecodeError`] complains about, given where the payload starts in the body.
fn payload_offset(payload_at: usize) -> usize {
    CHUNK_PAYLOAD_AT + payload_at
}

/// The byte a [`DecodeError`] points at, for the variants that name one.
fn error_offset(error: &DecodeError) -> usize {
    match error {
        DecodeError::Char { at, .. }
        | DecodeError::Padding { at }
        | DecodeError::TrailingBits { at } => *at,
        // A string that is not a whole number of groups is wrong as a whole, and an output buffer
        // too small for a payload the grammar bounds cannot happen: name the payload's start either
        // way rather than invent an offset the error did not report.
        DecodeError::Length(_) | DecodeError::OutputTooSmall { .. } => 0,
    }
}
