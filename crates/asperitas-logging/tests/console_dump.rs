//! Strictness suite for the audio-dump base64 codec in `asperitas_logging::dump`.
//!
//! Run with: `cargo test -p asperitas-logging`
//!
//! The device writes captured audio to the console transport as base64, and `sanitize_byte`
//! passes printable ASCII through untouched — so a corrupted payload arrives looking completely
//! legitimate. The only defence is a decoder that refuses everything the reference implementation
//! refuses, including the cases a permissive one waves through. That claim is worth nothing as
//! prose, so this file checks it against `base64 0.23` itself: same decisions, same bytes, over
//! random input, mutated input, and every possible final symbol of both tail shapes.
//!
//! Three ideas hold the design together, one section each:
//!
//! * **Equivalence.** Our encoder is byte-identical to the reference and our decoder agrees with
//!   it on accept/reject and on decoded bytes. Error *wording* is ours; the decision is comparable
//!   and is all either implementation can honestly be asked about.
//! * **Enumeration where sampling proves nothing.** Whether ignored bits are rejected is a property
//!   of 64 specific characters, so all 64 get checked for each tail length rather than hoped over.
//! * **Pinned vectors.** RFC 4648 §10 verbatim, the two saturation patterns at full payload size,
//!   and one input whose encoding is the alphabet itself — expected strings written down, never
//!   computed from the encoder under test.
//!
//! Style follows `tests/console_frame.rs`: strategies sized by *count*, lowercase `prop_assert!`
//! messages naming the offending values, and one property per banner.

use base64::Engine;
use proptest::prelude::*;

use asperitas_logging::dump::{
    self, encoded_len, max_raw_for, DecodeError, EncodeError, B64_ALPHABET,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Longest raw input the properties generate: one full dump payload (129 bytes, the chunk size the
/// grammar will pin in TASK-038.02.02) plus enough slack to reach past a padded group, so group
/// boundaries and tails both get exercised at realistic size.
const MAX_PROBE_RAW: usize = 136;

/// Output room a `MAX_PROBE_RAW` encoding needs — derived from the function under test only for
/// *sizing*, never for an expected value.
const MAX_PROBE_ENCODED: usize = encoded_len(MAX_PROBE_RAW);

/// What a decoder decided, reduced to the two things both implementations can be compared on.
///
/// Error types differ deliberately — the reference reports text, this codec reports variants with
/// offsets — so comparing them directly would test the wrong thing. Accept/reject and the bytes on
/// acceptance are the achievable contract.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Accepted(Vec<u8>),
    Refused,
}

/// This codec's verdict on `b64`, into a buffer sized by [`max_raw_for`] alone.
///
/// Sizing from `max_raw_for` is part of what's being tested: if that constant ever under-provides,
/// decoding real payloads starts failing with `OutputTooSmall` and these properties notice.
fn our_decode(b64: &[u8]) -> Verdict {
    let mut out = vec![0u8; max_raw_for(b64.len())];
    match dump::decode(b64, &mut out) {
        Ok(written) => Verdict::Accepted(out[..written].to_vec()),
        Err(_) => Verdict::Refused,
    }
}

/// The reference engine's verdict on `b64`, with padding required as the transport needs.
fn reference_decode(b64: &[u8]) -> Verdict {
    let mut out = vec![0u8; max_raw_for(b64.len())];
    match base64::engine::general_purpose::STANDARD.decode_slice(b64, &mut out) {
        Ok(written) => Verdict::Accepted(out[..written].to_vec()),
        Err(_) => Verdict::Refused,
    }
}

/// This codec's encoding of `raw`.
fn our_encode(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; encoded_len(raw.len())];
    let written = dump::encode(raw, &mut out).expect("buffer sized by encoded_len");
    out[..written].to_vec()
}

/// The reference engine's encoding of `raw`.
fn reference_encode(raw: &[u8]) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .encode(raw)
        .into_bytes()
}

/// Characters a mutation draws from, weighted toward the ones that decide strictness.
///
/// `'='` and non-alphabet bytes dominate because those are the inputs a lenient decoder is tempted
/// to repair; plain alphabet characters keep ordinary bit-flips in the sample. Mirrors the
/// delimiter-heavy weighting in `console_frame.rs`.
fn mutation_byte() -> impl Strategy<Value = u8> {
    prop_oneof![
        5 => Just(b'='),
        3 => Just(b'\n'),
        3 => Just(b' '),
        2 => Just(b'~'),
        2 => Just(b'*'),
        1 => Just(b'+'),
        1 => Just(b'/'),
        1 => Just(b'A'),
        1 => Just(b'z'),
        2 => any::<u8>(),
    ]
}

// ---------------------------------------------------------------------------
// Property 1: the encoder is byte-identical to the reference
// ---------------------------------------------------------------------------

proptest! {
    /// Every raw input encodes to exactly what `base64`'s `STANDARD` engine produces, and
    /// [`encoded_len`] predicts that length exactly.
    ///
    /// Byte equality rather than decode-round-trip equality matters: a decoder that accepts both
    /// our output and the reference's would hide an encoder emitting non-canonical padding, which
    /// is precisely the drift a host reassembler must not have to guess about.
    #[test]
    fn encodes_exactly_like_the_reference(raw in prop::collection::vec(any::<u8>(), 0..=MAX_PROBE_RAW)) {
        let ours = {
            let mut buf = [0u8; MAX_PROBE_ENCODED];
            let written = dump::encode(&raw, &mut buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            buf[..written].to_vec()
        };
        prop_assert_eq!(ours.len(), encoded_len(raw.len()), "encoded_len lied about {:?}", raw.len());
        prop_assert_eq!(&ours, &reference_encode(&raw), "encoder diverged for {} bytes", raw.len());
    }
}

// ---------------------------------------------------------------------------
// Property 2: the decoder agrees with the reference on what it accepts
// ---------------------------------------------------------------------------

proptest! {
    /// Whatever the reference accepted, we accept identically; whatever it refused, we refuse.
    /// Inputs here are well-formed by construction, which pins the accepting half of the contract.
    #[test]
    fn decodes_what_the_reference_encoded(
        raw in prop::collection::vec(any::<u8>(), 0..=MAX_PROBE_RAW),
    ) {
        let encoded = reference_encode(&raw);
        prop_assert_eq!(our_decode(&encoded), Verdict::Accepted(raw.clone()));
        prop_assert_eq!(reference_decode(&encoded), Verdict::Accepted(raw));
    }
}

// ---------------------------------------------------------------------------
// Property 3: the decoder and the reference refuse the same broken strings
// ---------------------------------------------------------------------------

proptest! {
    /// One to three characters of a valid encoding are replaced with delimiter-heavy bytes, and
    /// the two decoders still agree.
    ///
    /// This is the property the transport actually needs: corruption delivers strings, not error
    /// classes, and the only interesting question is whether a damaged payload reads as clean.
    /// Agreement here means no mutation slips past unnoticed on one side while the other catches
    /// it.
    #[test]
    fn rejects_or_accepts_mutation_exactly_like_the_reference(
        raw in prop::collection::vec(any::<u8>(), 1..=24usize),
        edits in prop::collection::vec((0..64usize, mutation_byte()), 1..=3usize),
    ) {
        let mut candidate = reference_encode(&raw);
        for (index, byte) in edits {
            let at = index % candidate.len();
            candidate[at] = byte;
        }
        let ours = our_decode(&candidate);
        let theirs = reference_decode(&candidate);
        prop_assert!(
            ours == theirs,
            "disagreement on {:?}: ours={:?} reference={:?}",
            String::from_utf8_lossy(&candidate),
            ours,
            theirs,
        );
    }
}

// ---------------------------------------------------------------------------
// Enumeration: every final symbol, for both tail shapes
// ---------------------------------------------------------------------------

/// A one-byte tail (`"xy=="`) puts six bits in its second symbol against eight per output byte, so
/// four of them are dead: only the symbols whose low nibble is zero (`A`, `Q`, `g`, `w`) are
/// canonical. Checking all 64 rather than
/// sampling is deliberate: whether ignored bits are tolerated is a property of individual
/// characters, and a single character missed here is a corrupted payload nobody ever sees.
#[test]
fn every_final_symbol_of_a_one_byte_tail_agrees_with_the_reference() {
    let mut accepted = 0usize;
    for (value, symbol) in B64_ALPHABET.iter().enumerate() {
        let candidate = [b'A', *symbol, b'=', b'='];
        let ours = our_decode(&candidate);
        assert_eq!(
            ours,
            reference_decode(&candidate),
            "disagreement on tail symbol {symbol:#04x} (value {value})"
        );
        let canonical = value & 0x0F == 0;
        match (canonical, &ours) {
            (true, Verdict::Accepted(_)) => accepted += 1,
            (false, Verdict::Refused) => {}
            _ => panic!(
                "symbol {symbol:#04x} (value {value}, low nibble {:#04x}) verdict {ours:?} \
                 contradicts canonical={canonical}",
                value & 0x0F,
            ),
        }
    }
    assert_eq!(
        accepted, 4,
        "only a sixteenth of the alphabet survives a 1-byte tail"
    );
}

/// A two-byte tail (`"xyz="`) leaves two dead bits in its third symbol, so 16 of the 64 symbols are
/// canonical. Enumerated for the reason stated on the one-byte case above.
#[test]
fn every_final_symbol_of_a_two_byte_tail_agrees_with_the_reference() {
    let mut accepted = 0usize;
    for (value, symbol) in B64_ALPHABET.iter().enumerate() {
        let candidate = [b'A', b'B', *symbol, b'='];
        let ours = our_decode(&candidate);
        assert_eq!(
            ours,
            reference_decode(&candidate),
            "disagreement on tail symbol {symbol:#04x} (value {value})"
        );
        let canonical = value & 0x03 == 0;
        match (canonical, &ours) {
            (true, Verdict::Accepted(_)) => accepted += 1,
            (false, Verdict::Refused) => {}
            _ => panic!(
                "symbol {symbol:#04x} (value {value}, low two bits {:#04x}) verdict {ours:?} \
                 contradicts canonical={canonical}",
                value & 0x03,
            ),
        }
    }
    assert_eq!(
        accepted, 16,
        "a quarter of the alphabet should survive a 2-byte tail"
    );
}

/// Full-size payloads take no padding — 129 is a multiple of 3 — so the tail rules above are
/// exercised only by a block's final chunk. This confirms that claim instead of leaving it as an
/// assumption in a comment: a full chunk is four-symbol groups start to finish.
#[test]
fn a_full_payload_needs_no_padding() {
    let raw = [0xA5u8; 129];
    let encoded = our_encode(&raw);
    assert_eq!(encoded.len(), 172);
    assert!(
        !encoded.contains(&b'='),
        "a 129-byte payload should pad nowhere"
    );
    assert_eq!(our_decode(&encoded), Verdict::Accepted(raw.to_vec()));
}

// ---------------------------------------------------------------------------
// Pinned vectors
// ---------------------------------------------------------------------------

/// RFC 4648 §10 in full, transcribed from the specification rather than produced by either
/// implementation. If these move, the codec is broken and nothing else in this file matters.
#[test]
fn matches_the_rfc_4648_vectors() {
    let vectors: [(&[u8], &[u8]); 7] = [
        (b"", b""),
        (b"f", b"Zg=="),
        (b"fo", b"Zm8="),
        (b"foo", b"Zm9v"),
        (b"foob", b"Zm9vYg=="),
        (b"fooba", b"Zm9vYmE="),
        (b"foobar", b"Zm9vYmFy"),
    ];
    for (raw, expected) in vectors {
        assert_eq!(
            our_encode(raw),
            expected.to_vec(),
            "encode mismatch for {raw:?}"
        );
        assert_eq!(
            reference_encode(raw),
            expected.to_vec(),
            "oracle moved for {raw:?}"
        );
        assert_eq!(
            our_decode(expected),
            Verdict::Accepted(raw.to_vec()),
            "decode mismatch for {expected:?}"
        );
    }
}

/// Both saturation patterns at exactly one full payload, pinned as literal wire text.
///
/// Zeros encode to nothing but `A` and ones to nothing but `/`; anything else at these sizes means
/// the group loop or the alphabet table is wrong in a way that would corrupt a whole capture
/// identically, which is the worst failure mode available.
#[test]
fn saturating_patterns_at_full_payload_size() {
    /// 172 `A`: every six-bit group of 129 zero bytes is zero.
    const ALL_ZEROS: &[u8] = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    /// 172 `/`: every six-bit group of 129 `0xFF` bytes is all-ones.
    const ALL_ONES: &[u8] = b"////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////";
    assert_eq!(ALL_ZEROS.len(), 172);
    assert_eq!(ALL_ONES.len(), 172);

    assert_eq!(our_encode(&[0x00u8; 129]), ALL_ZEROS.to_vec());
    assert_eq!(our_encode(&[0xFFu8; 129]), ALL_ONES.to_vec());
    assert_eq!(our_decode(ALL_ZEROS), Verdict::Accepted(vec![0x00u8; 129]));
    assert_eq!(our_decode(ALL_ONES), Verdict::Accepted(vec![0xFFu8; 129]));
}

/// Forty-eight bytes chosen so their encoding is the alphabet itself: the 64 six-bit groups of the
/// input are the values 0..=64 in order, which puts every symbol — including `+` and `/` — on the
/// wire exactly once.
///
/// The input is derived (once, offline) rather than guessed, but the expectation is the alphabet
/// constant, which is as independent of the encoder as an expectation can be.
#[test]
fn one_vector_covers_every_alphabet_symbol() {
    let raw: [u8; 48] = [
        0x00, 0x10, 0x83, 0x10, 0x51, 0x87, 0x20, 0x92, 0x8B, 0x30, 0xD3, 0x8F, 0x41, 0x14, 0x93,
        0x51, 0x55, 0x97, 0x61, 0x96, 0x9B, 0x71, 0xD7, 0x9F, 0x82, 0x18, 0xA3, 0x92, 0x59, 0xA7,
        0xA2, 0x9A, 0xAB, 0xB2, 0xDB, 0xAF, 0xC3, 0x1C, 0xB3, 0xD3, 0x5D, 0xB7, 0xE3, 0x9E, 0xBB,
        0xF3, 0xDF, 0xBF,
    ];
    let encoded = our_encode(&raw);
    assert_eq!(encoded, B64_ALPHABET.to_vec());
    assert_eq!(reference_encode(&raw), B64_ALPHABET.to_vec());
    assert!(
        encoded.contains(&b'+') && encoded.contains(&b'/'),
        "the +/ pair is the point"
    );
    assert_eq!(our_decode(B64_ALPHABET), Verdict::Accepted(raw.to_vec()));
}

// ---------------------------------------------------------------------------
// Refusal shapes: each malformed class, and undersized buffers
// ---------------------------------------------------------------------------

/// Every class of malformed input the strictness rules name, checked as a specific error carrying a
/// specific offset — never a panic, never a silent repair.
///
/// The classes overlap in the wild (junk after padding usually breaks the length rule first), so
/// each case below isolates one by keeping the string a whole number of groups long.
#[test]
fn refuses_each_malformed_class_at_its_offset() {
    let cases: &[(&[u8], DecodeError)] = &[
        // Not a whole number of groups.
        (b"A", DecodeError::Length(1)),
        (b"QUJDRA=", DecodeError::Length(7)),
        // Bytes outside the alphabet, including the whitespace the transport never emits.
        (b"\nAAA", DecodeError::Char { at: 0, byte: b'\n' }),
        (b" AAA", DecodeError::Char { at: 0, byte: b' ' }),
        (b"AA~A", DecodeError::Char { at: 2, byte: b'~' }),
        (b"AAA*", DecodeError::Char { at: 3, byte: b'*' }),
        (b"AAA\x80", DecodeError::Char { at: 3, byte: 0x80 }),
        // Padding where the grammar forbids it: too early, split from the end, or mid-string.
        (b"A===", DecodeError::Padding { at: 1 }),
        (b"====", DecodeError::Padding { at: 0 }),
        (b"QU=JDRA=", DecodeError::Padding { at: 2 }),
        (b"QUJDRA=B", DecodeError::Padding { at: 6 }),
        (b"AAAA=A==", DecodeError::Padding { at: 4 }),
        (b"QUJDRA==QUJD", DecodeError::Padding { at: 6 }),
        // Canonical characters whose ignored bits are set.
        (b"AB==", DecodeError::TrailingBits { at: 1 }),
        (b"q6==", DecodeError::TrailingBits { at: 1 }),
        (b"q69=", DecodeError::TrailingBits { at: 2 }),
    ];

    let mut out = [0u8; 64];
    for (input, expected) in cases {
        let shown = String::from_utf8_lossy(input);
        let got = dump::decode(input, &mut out);
        assert_eq!(
            &got.expect_err("must refuse"),
            expected,
            "wrong refusal for {shown:?}"
        );
    }
}

/// A hand-picked agreement table over the strings most likely to separate a lenient decoder from a
/// strict one: padding that is legal-shaped but semantically empty, tails whose ignored bits are set
/// or clear by one bit, the `+`/`/` pair at every position, and control bytes.
///
/// Only agreement is asserted, never a variant, so this stays honest if the reference's rules turn
/// out subtler than ours: disagreement is the finding, whichever side reports it. The mutation
/// property samples this space; these rows were chosen for it.
#[test]
fn agrees_with_the_reference_on_tricky_strings() {
    let cases: &[&[u8]] = &[
        b"AAAA====",
        b"AAAAAA==",
        b"A=AA",
        b"=AAA",
        b"AA=A",
        b"A===",
        b"====",
        b"AA==",
        b"AB==",
        b"AE==",
        b"AP/",
        b"AP//",
        b"//8=",
        b"////",
        b"ZZZ=",
        b"z/z/",
        b"AD/+Q=",
        b"\x00\x00\x00\x00",
        b"~~~~",
        b"QUlJ",
        b"aGVsbG8=",
        b"aGVsbG8",
        b"aGVsbG8==",
    ];
    for input in cases {
        let shown = String::from_utf8_lossy(input);
        assert_eq!(
            our_decode(input),
            reference_decode(input),
            "disagreement on {shown:?}"
        );
    }
}

/// Undersized output is reported as an error naming what it needed, before writing anything, in
/// both directions.
///
/// A dump writer recycles one buffer across chunks, so a short buffer has to be a clean refusal
/// with the required size attached — a partial write would leave the previous chunk's tail behind
/// and read as valid audio.
#[test]
fn refuses_undersized_buffers_without_writing() {
    let mut short = [0xA5u8; 3];
    assert_eq!(
        dump::encode(b"abcd", &mut short),
        Err(EncodeError::OutputTooSmall { need: 8 })
    );
    assert_eq!(short, [0xA5; 3], "encode wrote into a buffer it refused");

    let mut short = [0xA5u8; 3];
    assert_eq!(
        dump::decode(b"QUJDRA==", &mut short),
        Err(DecodeError::OutputTooSmall { need: 4 })
    );
    assert_eq!(short, [0xA5; 3], "decode wrote into a buffer it refused");

    // Exact fit is accepted; the boundary is inclusive.
    let mut exact = [0u8; 4];
    assert_eq!(dump::decode(b"QUJDRA==", &mut exact), Ok(4));
    assert_eq!(exact, *b"ABCD");

    // An empty output buffer is legal for empty input and only for empty input.
    let mut none = [];
    assert_eq!(dump::decode(b"", &mut none), Ok(0));
    assert_eq!(
        dump::decode(b"QUJD", &mut none),
        Err(DecodeError::OutputTooSmall { need: 3 })
    );
}

/// The two size functions agree with the reference across every length the codec will see, so a
/// caller sizing a buffer from them cannot be short by a character.
#[test]
fn size_arithmetic_holds_at_every_length() {
    for raw_len in 0..=MAX_PROBE_RAW {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i * 7 + 3) as u8).collect();
        let encoded = reference_encode(&raw);
        assert_eq!(
            encoded_len(raw_len),
            encoded.len(),
            "encoded_len wrong at {raw_len}"
        );
        assert_eq!(
            max_raw_for(encoded.len()),
            raw_len.div_ceil(3) * 3,
            "max_raw_for wrong at {raw_len}"
        );
        assert_eq!(our_decode(&encoded), Verdict::Accepted(raw));
    }
}
