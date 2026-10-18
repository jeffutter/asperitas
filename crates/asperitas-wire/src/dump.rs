//! Audio-dump payload layer — the binary-to-text half of getting captured samples off the device.
//!
//! Captured audio has to travel through the console transport in [`crate::frame`], which is
//! printable-ASCII text: [`crate::frame::sanitize_byte`] replaces every control byte, so raw sample
//! bytes cannot go near it. This module supplies the codec that makes them fit — RFC 4648 standard
//! base64 from the `base64` crate's `STANDARD` engine (`no_std`, allocation-free through its slice
//! API), which is strict enough that silent corruption cannot survive a round trip.
//!
//! On top of the codec sits the record grammar that carries these payloads — `AUDIO` and `AUDEND`
//! bodies, the chunk geometry derived from them, and one composite entry point per record kind — so
//! the efficiency figures quoted in documentation are the ones the encoder produces, not the ones a
//! plan hoped for. See *Record grammar* below.
//!
//! The other direction - reassembling blocks on the host and proving them complete - lives in
//! `asperitas-host`, which reads the templates and field widths published here.
//!
//! # Why this module is not behind `log-usb`
//!
//! Ungated for the same reason as [`crate::frame`]: it is pure byte arithmetic with no
//! transport, so the host tools and the tests link exactly what the firmware does.
//!
//! # Why base64 and not something denser
//!
//! Every candidate has to survive `sanitize_byte` unchanged, and the transport reserves bytes for
//! its own protocol:
//!
//! | encoding | raw per 4 wire bytes | verdict |
//! |---|---|---|
//! | hex | 2 | survives sanitisation, but spends 50% more wire than base64 for nothing gained |
//! | Ascii85 / Z85 | ~5 | denser, but those alphabets contain `~` — the record start marker — and `<>`, which the control channel reserves host→device. A corrupted body could then be reassembled into a fake record boundary |
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
//! through. The `STANDARD` engine's default configuration is exactly this contract:
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
//! `tests/console_dump.rs` pins the accept/reject decision and the [`DecodeError`] variant for each
//! class, plus RFC 4648 and saturation vectors written down rather than computed. Error *variants*
//! are this module's own; the engine's wording is not exposed.
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
//! Sizes fall out of [`crate::frame::MAX_BODY`], which bounds the body *including* keys:
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
//! free. Full budget table and rejected alternatives: the table above.
//!
//! **Sample bytes cannot be mangled by framing**, because they are base64 before they are framed: the
//! alphabet is printable ASCII, so [`crate::frame::sanitize_byte`] has nothing to substitute. That is
//! a structural property rather than a lucky accident, and
//! `tests/console_dump.rs::round_trips_through_the_real_decoder` exercises it with payloads holding
//! every byte a substitution would target.
//!
//! **Block size: up to [`MAX_CHUNKS_PER_BLOCK`] (255) chunks** — the most the `n` field can name,
//! 32,895 raw bytes. The rig's 32 KiB ring block uses all of them (see `capture`), so one lost
//! block costs one ring block of measurement rather than the whole capture.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use log::Level;

use crate::body::BodyWriter;
use crate::frame::{
    self, check_decimal_fits, check_hex_width, Encoded, MAX_BODY, MAX_FRAME, PREFIX_LEN,
    TRAILER_LEN,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why an encoding attempt could not write its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// `out` holds fewer than `need` bytes; nothing was written.
    OutputTooSmall { need: usize },
}

/// Why a base64 string was refused. Offsets are reported where the underlying engine knows them,
/// so a caller can say *where* a capture went wrong rather than only *that* it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Input length is not a whole number of 4-character groups.
    Length(usize),
    /// `byte` at `at` is not an alphabet symbol.
    Char { at: usize, byte: u8 },
    /// `=` sits where the grammar forbids it: inside a group, split from the end, outnumbering
    /// the bytes it stands for, or missing where it is mandatory. The engine does not say where.
    Padding,
    /// The final symbol at `at` is a legitimate character whose unused low bits are set, so the
    /// string is not the canonical encoding of any byte sequence.
    TrailingBits { at: usize },
    /// `out` holds fewer than `need` decoded bytes. Its contents are unspecified.
    OutputTooSmall { need: usize },
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
// Encoding and decoding
// ---------------------------------------------------------------------------

/// Encode `raw` into `out`, returning the number of characters written.
///
/// Output is canonical and padded (`base64`'s `STANDARD` engine). A short output buffer is
/// reported before anything is written, leaving `out` untouched.
pub fn encode(raw: &[u8], out: &mut [u8]) -> Result<usize, EncodeError> {
    let need = encoded_len(raw.len());
    if out.len() < need {
        return Err(EncodeError::OutputTooSmall { need });
    }
    match STANDARD.encode_slice(raw, out) {
        Ok(written) => Ok(written),
        Err(base64::EncodeSliceError::OutputSliceTooSmall) => {
            unreachable!("output length checked against encoded_len above")
        }
    }
}

/// Decode a base64 string into `out`, returning the number of raw bytes written.
///
/// Strict in every direction described at the top of this module. When `out` is too small the
/// error names the room needed and the contents of `out` are unspecified.
pub fn decode(b64: &[u8], out: &mut [u8]) -> Result<usize, DecodeError> {
    if !b64.len().is_multiple_of(4) {
        return Err(DecodeError::Length(b64.len()));
    }
    match STANDARD.decode_slice(b64, out) {
        Ok(written) => Ok(written),
        Err(base64::DecodeSliceError::OutputSliceTooSmall) => {
            let padding = b64.iter().rev().take(2).take_while(|b| **b == b'=').count();
            Err(DecodeError::OutputTooSmall {
                need: max_raw_for(b64.len()) - padding,
            })
        }
        Err(base64::DecodeSliceError::DecodeError(error)) => Err(match error {
            base64::DecodeError::InvalidByte(_, b'=') | base64::DecodeError::InvalidPadding => {
                DecodeError::Padding
            }
            base64::DecodeError::InvalidByte(at, byte) => DecodeError::Char { at, byte },
            base64::DecodeError::InvalidLength(_) => DecodeError::Length(b64.len()),
            base64::DecodeError::InvalidLastSymbol { offset, .. } => {
                DecodeError::TrailingBits { at: offset }
            }
        }),
    }
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
pub const AUDIO_PREFIX: &[u8] = b"AUDIO blk=";
/// ` n=` — chunk-count separator, shared by both record kinds so the two cannot disagree about it.
pub const SEP_N: &[u8] = b" n=";
/// ` c=` — chunk-index separator.
pub const SEP_C: &[u8] = b" c=";
/// ` d=` — payload separator; everything after it is base64.
pub const SEP_D: &[u8] = b" d=";
/// `AUDEND blk=` — opens a block-summary body.
pub const AUDEND_PREFIX: &[u8] = b"AUDEND blk=";
/// ` bytes=` — total-raw-bytes separator.
pub const SEP_BYTES: &[u8] = b" bytes=";
/// ` crc16=` — block-checksum separator.
pub const SEP_CRC16: &[u8] = b" crc16=";

/// Digits in `blk`: 65,536 ids before the field wraps.
pub const BLK_HEX_DIGITS: usize = 4;
/// Digits in `n` and `c`.
pub const COUNT_HEX_DIGITS: usize = 2;
/// Digits in `crc16` — the same width as the frame trailer, so the two checksums look alike on the
/// wire and one host regex reads both.
pub const CRC_HEX_DIGITS: usize = 4;

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
pub const COUNT_FIELD_MODULUS: usize = 1 << (COUNT_HEX_DIGITS * 4);

/// Most chunks one block may hold: 255 — one less than the field's modulus, because 256 has no
/// two-digit representation and emitting it would write `00`, handing the assembler a block that
/// claims no chunks at all. The 256 in older budget notes means this modulus; the capacity
/// is what a writer must stay inside.
pub const MAX_CHUNKS_PER_BLOCK: usize = COUNT_FIELD_MODULUS - 1;

/// Largest raw payload one block can describe, hence the widest value `bytes` can honestly carry.
pub const MAX_BLOCK_BYTES: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// Decimal digits in `bytes`: exactly enough for [`MAX_BLOCK_BYTES`], so widening the chunk geometry
/// widens the field with it.
pub const BYTES_DEC_DIGITS: usize = decimal_width(MAX_BLOCK_BYTES as u32);

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

// The pins. Each one restates a figure this module's documentation and the rig docs publish; if a
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
#[deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
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

    let mut w = BodyWriter::new(&mut out[..]);
    w.put(AUDIO_PREFIX);
    w.hex(block_index % BLOCK_ID_MODULUS, BLK_HEX_DIGITS);
    w.put(SEP_N);
    w.hex(chunks as u32, COUNT_HEX_DIGITS);
    w.put(SEP_C);
    w.hex(chunk_index as u32, COUNT_HEX_DIGITS);
    w.put(SEP_D);

    // The budget check above means this cannot fail; mapping rather than unwrapping keeps the
    // refusal path free of panics regardless.
    let too_long = || BodyError::ChunkTooLong { len: raw.len() };
    let payload = w.rest_mut().get_mut(..chars).ok_or_else(too_long)?;
    let written = encode(raw, payload).map_err(|_| too_long())?;
    debug_assert_eq!(written, chars);
    w.advance(written);
    Ok(w.filled())
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

    let mut w = BodyWriter::new(&mut out[..]);
    w.put(AUDEND_PREFIX);
    w.hex(block_index % BLOCK_ID_MODULUS, BLK_HEX_DIGITS);
    w.put(SEP_N);
    w.hex(chunks as u32, COUNT_HEX_DIGITS);
    w.put(SEP_BYTES);
    w.decimal(total_bytes, BYTES_DEC_DIGITS);
    w.put(SEP_CRC16);
    w.hex(u32::from(crc), CRC_HEX_DIGITS);
    Ok(w.filled())
}

/// Build and frame a complete `AUDIO` record, returning what landed in `frame`.
///
/// The composite callers should reach for: it does the body and [`frame::encode`] in one step, so no
/// caller re-derives the two-buffer dance. Both buffers are explicit rather than a hidden static
/// because the device supplies `frame` from the existing `RECORD_BUFS` (the lock-guarded
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
