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
    self, encoded_len, max_raw_for, BodyError, DecodeError, EncodeError, B64_ALPHABET,
};
use asperitas_logging::frame::{
    crc16_ccitt, Decoder, Encoded, Stats, MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN,
};
use asperitas_logging::Level;

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

// ---------------------------------------------------------------------------
// Record grammar: pinned frames, constant header, refusals, published arithmetic
// ---------------------------------------------------------------------------
//
// Everything in this section checks `AUDIO`/`AUDEND` bodies against something the encoder did not
// compute: hand-written wire bytes, CRCs produced by a separate implementation (CPython's, quoted in
// each row's doc comment), the catalogue's own CRC-16/CCITT-FALSE check value, and arithmetic done on
// paper in TASK-038.02's Implementation Notes. Two of our own functions agreeing with each other
// proves only that they share a mistake.

/// Width of the `crc16` field on the wire.
const CRC_HEX_ON_WIRE: usize = 4;

/// Full wire bytes for a 129-byte chunk: block `0x1234`, 64 chunks in it, this is chunk 0, seq
/// `0x42`, `t_ms` 4567.
///
/// The payload is CPython's encoding of `bytes(range(129))` (raw `0x00..=0x80`) and `8500` is
/// CRC-16/CCITT-FALSE over `wire[1..len - 7]`:
///
/// ```text
/// python3 -c 'import base64;print(base64.b64encode(bytes(range(129))).decode())'
/// ```
///
/// followed by the CCITT-FALSE loop (poly `0x1021`, init `0xFFFF`, no reflection) applied to the
/// assembled frame. 27 header + 172 payload = 199 body, 227 wire bytes.
const GOLDEN_FULL_CHUNK: &[u8] = concat!(
    "~I 00000042 00004567 AUDIO blk=1234 n=40 c=00 d=",
    "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKi",
    "ssLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVW",
    "V1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn+A*8500\r\n",
)
.as_bytes();

/// Same record kind, final chunk of its block: 7 raw bytes, i.e. `7 % 3 == 1`, so the last group is
/// one symbol plus `==`. Raw bytes deliberately include `0x00`, `0x7F` and `0xFF` — the values a
/// transport that mangles bytes would mangle.
const GOLDEN_TAIL_ONE_BYTE: &[u8] =
    b"~I 00000043 00004568 AUDIO blk=1234 n=40 c=3f d=8J9hAH//XA==*dd85\r\n";

/// And `8 % 3 == 2`: three symbols plus one `=`.
const GOLDEN_TAIL_TWO_BYTES: &[u8] =
    b"~I 00000044 00004569 AUDIO blk=0001 n=02 c=01 d=/wD+Af0C/AM=*8f07\r\n";

/// A closing `AUDEND` for a three-chunk block whose raw bytes were `123456789`.
///
/// `crc16=29b1` is not ours either: `0x29b1` is the published check value for CRC-16/CCITT-FALSE,
/// which is defined as the checksum of the ASCII string `123456789` (`frame.rs`'s own parameter
/// table says so). The block summary therefore carries a payload checksum with an outside witness,
/// and `238c` is the frame checksum of the row as written.
const GOLDEN_AUDEND: &[u8] =
    b"~I 00000045 00004570 AUDEND blk=0000 n=03 bytes=00009 crc16=29b1*238c\r\n";

/// Build an `AUDIO` body, failing loudly if the chosen fields were invalid.
fn audio_body_bytes(block_index: u32, chunks: u16, chunk_index: u16, raw: &[u8]) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let len = dump::audio_body(block_index, chunks, chunk_index, raw, &mut body)
        .expect("fields chosen by these tests are valid");
    body[..len].to_vec()
}

/// Build an `AUDEND` body.
fn audend_body_bytes(block_index: u32, chunks: u16, total_bytes: u32, crc: u16) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let len = dump::audend_body(block_index, chunks, total_bytes, crc, &mut body)
        .expect("fields chosen by these tests are valid");
    body[..len].to_vec()
}

/// Frame a complete `AUDIO` record, keeping what the encoder reported about it.
fn encoded_audio(
    raw: &[u8],
    block_index: u32,
    chunks: u16,
    chunk_index: u16,
) -> (Vec<u8>, Encoded) {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audio_record(
        Level::Info,
        0x42,
        4567,
        block_index,
        chunks,
        chunk_index,
        raw,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    (frame_buf[..enc.len].to_vec(), enc)
}

/// The same with the fields a golden row needs, minus the bookkeeping.
fn audio_wire(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    chunk_index: u16,
    raw: &[u8],
) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audio_record(
        level,
        seq,
        now_ms,
        block_index,
        chunks,
        chunk_index,
        raw,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    frame_buf[..enc.len].to_vec()
}

/// `AUDEND` counterpart of [`audio_wire`].
fn audend_wire(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audend_record(
        level,
        seq,
        now_ms,
        block_index,
        chunks,
        total_bytes,
        crc,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    frame_buf[..enc.len].to_vec()
}

/// Check a frame's checksum digits against a literal that came from elsewhere.
///
/// Both directions matter: the recomputed CRC must equal the literal, and the digits on the wire must
/// spell that literal in lowercase. Reading only the first would let a row whose digits were edited
/// still pass if the edit landed in the covered range too.
fn assert_pinned_crc(wire: &[u8], expected: u16) {
    let body_end = wire.len() - TRAILER_LEN;
    assert_eq!(
        crc16_ccitt(&wire[1..body_end]),
        expected,
        "pinned CRC no longer describes this row — recompute the literal, do not paste ours"
    );
    assert_eq!(
        &wire[body_end..body_end + 5],
        format!("*{expected:04x}").as_bytes(),
        "checksum digits must be lowercase, matching `parse_hex`"
    );
}

/// One validated record as the decoder delivered it, owned so it outlives the decoder's borrow.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    level: u8,
    seq: u32,
    t_ms: u32,
    body: Vec<u8>,
}

/// Decode a whole stream with the real [`Decoder`], collecting validated records.
fn decode_stream(bytes: &[u8]) -> (Vec<Seen>, Stats) {
    let mut decoder = Decoder::new();
    let mut records = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let consumed = decoder.push(&bytes[pos..]);
        assert_ne!(
            consumed, 0,
            "decoder accepted nothing while records sat undelivered"
        );
        pos += consumed;
        while let Some(record) = decoder.next_record() {
            records.push(Seen {
                level: record.level,
                seq: record.seq,
                t_ms: record.t_ms,
                body: record.body.to_vec(),
            });
        }
    }
    decoder.finish();
    (records, decoder.stats())
}

/// Payload of an `AUDIO` body, taken at the offset the geometry claims and decoded strictly.
fn payload_of(body: &[u8]) -> Vec<u8> {
    assert!(
        body.starts_with(b"AUDIO blk="),
        "body does not open with the template it was built from: {body:?}"
    );
    let b64 = &body[dump::AUDIO_HEADER_LEN..];
    let mut raw = vec![0u8; max_raw_for(b64.len())];
    let written = dump::decode(b64, &mut raw).expect("a body we emitted decodes");
    raw[..written].to_vec()
}

/// A full chunk produces the pinned wire bytes, and the pinned CRC still describes them.
#[test]
fn golden_full_chunk_frame_is_reproduced_byte_for_byte() {
    let raw: Vec<u8> = (0..=128u8).collect();
    assert_eq!(raw.len(), dump::CHUNK_RAW);

    let body = audio_body_bytes(0x1234, 64, 0, &raw);
    assert_eq!(
        body.len(),
        dump::FULL_AUDIO_BODY_LEN,
        "a full chunk body is 199 bytes, one under MAX_BODY"
    );
    assert_eq!(dump::FULL_AUDIO_BODY_LEN, MAX_BODY - 1);
    assert_eq!(body, &GOLDEN_FULL_CHUNK[21..21 + body.len()]);

    let (wire, enc) = encoded_audio(&raw, 0x1234, 64, 0);
    assert_eq!(wire, GOLDEN_FULL_CHUNK);
    assert_eq!(wire.len(), dump::FULL_AUDIO_FRAME_LEN);
    assert_eq!(dump::FULL_AUDIO_FRAME_LEN, PREFIX_LEN + 199 + TRAILER_LEN);
    assert!(
        !enc.truncated,
        "a full chunk fits: truncation here means the budget moved"
    );
    assert_pinned_crc(GOLDEN_FULL_CHUNK, 0x8500);
}

/// Short final chunks reach the wire with their padding intact, for both tail shapes.
///
/// Only a block's last chunk exercises `=`, because 129 is a multiple of 3 — so this row pair is the
/// grammar's entire exposure to padding, and it has to survive framing.
#[test]
fn golden_padded_final_chunks_survive_framing() {
    let seven = [0xF0u8, 0x9F, 0x61, 0x00, 0x7F, 0xFF, 0x5C];
    let eight = [0xFFu8, 0x00, 0xFE, 0x01, 0xFD, 0x02, 0xFC, 0x03];

    let first = audio_wire(Level::Info, 0x43, 4568, 0x1234, 64, 63, &seven);
    assert_eq!(first, GOLDEN_TAIL_ONE_BYTE);
    assert_pinned_crc(GOLDEN_TAIL_ONE_BYTE, 0xdd85);

    let second = audio_wire(Level::Info, 0x44, 4569, 0x0001, 2, 1, &eight);
    assert_eq!(second, GOLDEN_TAIL_TWO_BYTES);
    assert_pinned_crc(GOLDEN_TAIL_TWO_BYTES, 0x8f07);

    // Framing sanitises body bytes, and neither `_` nor a lost pad character may appear: sample bytes
    // are base64 before they are framed, so substitution is impossible rather than merely absent.
    for wire in [&first, &second] {
        assert!(
            !wire.contains(&b'_'),
            "sanitisation altered a base64 payload"
        );
        assert_eq!(
            wire.len(),
            PREFIX_LEN + dump::AUDIO_HEADER_LEN + dump::encoded_len(7) + TRAILER_LEN,
            "header length must not depend on payload length"
        );
    }
}

/// A block summary reproduces its pinned row, including a checksum nobody here invented.
#[test]
fn golden_audend_frame_is_reproduced_byte_for_byte() {
    assert_eq!(
        crc16_ccitt(b"123456789"),
        0x29b1,
        "the catalogue check value is the reference for this row's payload CRC"
    );

    let body = audend_body_bytes(0x0000, 3, 9, 0x29b1);
    assert_eq!(body, b"AUDEND blk=0000 n=03 bytes=00009 crc16=29b1");
    assert_eq!(
        body.len(),
        dump::MAX_AUDEND_BODY_LEN,
        "with every numeric field at full width this is the longest AUDEND body"
    );

    let wire = audend_wire(Level::Info, 0x45, 4570, 0x0000, 3, 9, 0x29b1);
    assert_eq!(wire, GOLDEN_AUDEND);
    assert_eq!(wire.len(), PREFIX_LEN + 43 + TRAILER_LEN);
    assert_pinned_crc(GOLDEN_AUDEND, 0x238c);
}

// A constant 27-byte header for every legal field combination, so the payload always starts where
// the geometry says it does.
proptest! {
    #[test]
    fn header_is_constant_length_for_every_valid_field_combination(
        block_index in any::<u32>(),
        chunks in 1u16..=255u16,
        drawn_index in any::<u16>(),
        payload_len in 1usize..=dump::CHUNK_RAW,
    ) {
        // Folded rather than generated so legality cannot fail the property: the point here is what
        // the header does, not that illegal inputs are refused (that has its own test).
        let chunk_index = drawn_index % chunks;
        let raw: Vec<u8> = (0..payload_len).map(|i| (i * 31 + 7) as u8).collect();

        let body = audio_body_bytes(block_index, chunks, chunk_index, &raw);
        let expected_body = dump::AUDIO_HEADER_LEN + encoded_len(payload_len);
        if body.len() != expected_body {
            prop_assert!(
                false,
                "body length drifted from the fixed header at blk={:#x} n={} c={} len={}: {} != {}",
                block_index,
                chunks,
                chunk_index,
                payload_len,
                body.len(),
                expected_body
            );
        }
        prop_assert_eq!(
            &body[dump::AUDIO_HEADER_LEN - 3..dump::AUDIO_HEADER_LEN],
            b" d=",
            "payload must begin exactly where the header ends"
        );
        // Four separators, four spaces, wherever the values land: no field value leaks a separator
        // into the body and shifts where the payload starts.
        prop_assert_eq!(
            body[..dump::AUDIO_HEADER_LEN]
                .iter()
                .filter(|b| **b == b' ')
                .count(),
            4
        );

        if payload_len == dump::CHUNK_RAW {
            let (_, enc) = encoded_audio(&raw, block_index, chunks, chunk_index);
            prop_assert_eq!(enc.len, dump::FULL_AUDIO_FRAME_LEN);
            prop_assert!(!enc.truncated);
        }
    }
}

/// Each oversized or empty input gets its own error, and the caller's buffers stay untouched.
///
/// Refusal is the contract: a shortened chunk reassembles into audio that plays back fine and
/// measures wrong, which is the failure this codec exists to make impossible.
#[test]
fn refuses_instead_of_truncating() {
    let mut body = [0xA5u8; MAX_BODY];
    let one = [0x5Au8; 1];
    let too_long = [0x5Au8; dump::CHUNK_RAW + 1];

    // 256 is the interesting boundary: the field's modulus, not its capacity. Accepting it would
    // write `n=00` and hand the assembler a block that claims no chunks at all.
    assert_eq!(
        dump::audio_body(0, 256, 0, &one, &mut body),
        Err(BodyError::TooManyChunks { chunks: 256 })
    );
    assert_eq!(
        dump::audio_body(0, 3, 3, &one, &mut body),
        Err(BodyError::ChunkIndexOutOfRange {
            chunk_index: 3,
            chunks: 3
        }),
        "a chunk index at or past the count names a chunk the block will never send"
    );
    assert_eq!(
        dump::audio_body(0, 0, 0, &one, &mut body),
        Err(BodyError::ChunkIndexOutOfRange {
            chunk_index: 0,
            chunks: 0
        }),
        "a zero-chunk block has no chunk zero"
    );
    assert_eq!(
        dump::audio_body(0, 1, 0, &[], &mut body),
        Err(BodyError::EmptyChunk)
    );
    assert_eq!(
        dump::audio_body(0, 1, 0, &too_long, &mut body),
        Err(BodyError::ChunkTooLong {
            len: dump::CHUNK_RAW + 1
        })
    );

    // The summary refuses the same ways, and additionally guards the decimal field.
    assert_eq!(
        dump::audend_body(0, 256, 9, 0x29b1, &mut body),
        Err(BodyError::TooManyChunks { chunks: 256 })
    );
    assert_eq!(
        dump::audend_body(0, 3, (dump::MAX_BLOCK_BYTES + 1) as u32, 0x29b1, &mut body),
        Err(BodyError::BlockTooLarge {
            total_bytes: dump::MAX_BLOCK_BYTES + 1
        })
    );
    assert_eq!(
        body, [0xA5u8; MAX_BODY],
        "a refusal wrote into the body buffer"
    );

    // The composites propagate instead of framing a half-built record.
    let mut frame_buf = [0x3Cu8; MAX_FRAME];
    assert_eq!(
        dump::audio_record(
            Level::Info,
            1,
            1,
            0,
            256,
            0,
            &one,
            &mut body,
            &mut frame_buf
        ),
        Err(BodyError::TooManyChunks { chunks: 256 })
    );
    assert_eq!(
        dump::audend_record(Level::Info, 1, 1, 0, 256, 9, 0, &mut body, &mut frame_buf),
        Err(BodyError::TooManyChunks { chunks: 256 })
    );
    assert_eq!(
        frame_buf, [0x3Cu8; MAX_FRAME],
        "a refused record left bytes in the frame buffer"
    );
}

/// The efficiency figures documentation publishes are arithmetic on the shipped constants.
///
/// This is AC #6 made mechanical: change a template, a field width, or `MAX_BODY`, and one of these
/// lines fails and names the document that went stale. The prose form of each claim lives in
/// `dump`'s module doc and TASK-038.06's rig notes.
///
/// For the record: TASK-038.02 originally assumed 150 raw bytes per record. That was never reachable
/// — `MAX_BODY` bounds the whole body, keys included — and the corrected budget table is in
/// TASK-038.02's Implementation Notes.
#[test]
fn published_efficiency_matches_the_encoder() {
    // Useful fraction: 129 raw bytes inside a 227-byte frame.
    assert_eq!(
        (dump::CHUNK_RAW as u32 * 1_000) / dump::FULL_AUDIO_FRAME_LEN as u32,
        568,
        "documentation says 0.568 useful bytes per wire byte"
    );

    // Mono 16-bit capture at 48 kHz is 96,000 B/s; a chunk holds CHUNK_RAW bytes.
    let capture_bytes_per_s: u32 = 96_000;
    let records_per_s = capture_bytes_per_s.div_ceil(dump::CHUNK_RAW as u32);
    assert_eq!(
        records_per_s, 745,
        "documentation says a full-rate mono capture needs 745 records/s"
    );

    let wire_bytes_per_s = records_per_s * dump::FULL_AUDIO_FRAME_LEN as u32;
    assert_eq!(
        wire_bytes_per_s, 169_115,
        "documentation says ~169 kB/s of console traffic for mono 16-bit capture"
    );

    // 32-bit capture doubles the sample rate in bytes and the record rate with it.
    assert_eq!(
        2 * wire_bytes_per_s,
        338_230,
        "documentation says ~338 kB/s for mono 32-bit capture"
    );

    // One block at the recommended 64 chunks, and the ceiling the grammar can describe at all.
    assert_eq!(
        64 * dump::CHUNK_RAW,
        8_256,
        "TASK-038.03's recommended block"
    );
    assert_eq!(
        dump::MAX_BLOCK_BYTES,
        32_895,
        "`bytes` is sized for exactly this, so widening chunk geometry widens the field"
    );
}

/// Real decoder, real framing: bodies come back intact and `blk` tells interleaved blocks apart.
///
/// Two blocks are sent chunk-interleaved with every field identical except the block id, which is
/// the only thing that can possibly separate them. One payload contains every byte
/// `sanitize_byte` would substitute, so a body that comes back decoding to those bytes proves
/// framing never touched the samples.
#[test]
fn round_trips_through_the_real_decoder() {
    let low: Vec<u8> = (0..=128u8).collect();
    let high: Vec<u8> = (0..=128u8).rev().collect();
    // Every control byte, plus DEL: 33 + 1 values a transport is tempted to rewrite.
    let hostile: Vec<u8> = (0u8..=0x20).chain(std::iter::once(0x7F)).collect();
    assert_eq!(hostile.len(), 34);

    let crc_low = crc16_ccitt(&[low.as_slice(), hostile.as_slice()].concat());
    let crc_high = crc16_ccitt(&[high.as_slice(), hostile.as_slice()].concat());

    let mut stream = Vec::new();
    stream.extend(audio_wire(Level::Info, 1, 1_000, 0x1234, 2, 0, &low));
    stream.extend(audio_wire(Level::Info, 2, 1_001, 0x1235, 2, 0, &high));
    stream.extend(audio_wire(Level::Info, 3, 1_002, 0x1234, 2, 1, &hostile));
    stream.extend(audio_wire(Level::Info, 4, 1_003, 0x1235, 2, 1, &hostile));
    stream.extend(audend_wire(
        Level::Info,
        5,
        1_004,
        0x1234,
        2,
        (low.len() + hostile.len()) as u32,
        crc_low,
    ));
    stream.extend(audend_wire(
        Level::Info,
        6,
        1_005,
        0x1235,
        2,
        (high.len() + hostile.len()) as u32,
        crc_high,
    ));

    let (records, stats) = decode_stream(&stream);
    assert_eq!(records.len(), 6);
    assert_eq!(stats.records, 6);
    assert_eq!(stats.bad_frames, 0, "nothing we wrote looked broken");
    assert_eq!(stats.resyncs, 0);
    assert_eq!(stats.discarded_bytes, 0);

    // Payloads land byte-identical, including the hostile one.
    assert_eq!(payload_of(&records[0].body), low);
    assert_eq!(payload_of(&records[1].body), high);
    assert_eq!(payload_of(&records[2].body), hostile);
    assert_eq!(payload_of(&records[3].body), hostile);

    // Interleaving is legible: same `n`, same `c`, same payload, different block.
    assert_ne!(records[2].body, records[3].body);
    assert_eq!(
        &records[2].body[14..],
        &records[3].body[14..],
        "only `blk` differs"
    );
    assert_eq!(&records[2].body[10..14], b"1234");
    assert_eq!(&records[3].body[10..14], b"1235");

    // Levels and sequence numbers ride through unchanged.
    let letters: Vec<u8> = records.iter().map(|r| r.level).collect();
    assert_eq!(letters, [b'I'; 6], "data records travel at Info");
    let seqs: Vec<u32> = records.iter().map(|r| r.seq).collect();
    assert_eq!(seqs, [1, 2, 3, 4, 5, 6]);

    // The summaries carry the checksums over raw concatenation, so the host can verify a reassembled
    // block against the bytes rather than against the text that carried them.
    for (record, expected) in [(&records[4], crc_low), (&records[5], crc_high)] {
        let summary = record
            .body
            .strip_prefix(b"AUDEND blk=")
            .expect("summary body");
        // Fixed-width fields put the checksum digits at a fixed offset from the end.
        let digits = summary.len() - CRC_HEX_ON_WIRE;
        assert!(
            summary[..digits].ends_with(b" crc16="),
            "crc16 label missing from: {}",
            String::from_utf8_lossy(summary)
        );
        assert_eq!(
            &summary[digits..],
            format!("{expected:04x}").as_bytes(),
            "the wire must carry the CRC of the raw block bytes, lowercase"
        );
    }
}
