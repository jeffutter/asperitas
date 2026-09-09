//! Audio-dump payload layer — the binary-to-text half of getting captured samples off the device.
//!
//! Captured audio has to travel through the console transport in [`crate::frame`], which is
//! printable-ASCII text: [`crate::frame::sanitize_byte`] replaces every control byte, so raw sample
//! bytes cannot go near it. This module supplies the codec that makes them fit — RFC 4648 standard
//! base64, written for `no_std`, allocation-free, and strict enough that silent corruption cannot
//! survive a round trip.
//!
//! The record grammar that carries these payloads (`AUDIO`/`AUDEND`) builds on this codec and is
//! deliberately not here yet: with only the codec in place, its correctness is provable against a
//! reference implementation with no framing, no pipe, and no board involved.
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
