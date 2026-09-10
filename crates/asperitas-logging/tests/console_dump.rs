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
//! The last section moves up a level and drives `dump::BlockAssembler` over whole streams: which
//! record goes missing and which byte gets corrupted are generated rather than hand-picked, because
//! "we tested deleting a record" is weak evidence for "no record can go missing unnoticed".
//!
//! A closing section drops to the other end of the device path and checks the pipe headroom rule:
//! exhaustively, against a real `embassy_sync` ring, that a dump commit never takes the bytes
//! reserved for one maximum-size log record.
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

// ---------------------------------------------------------------------------
// Block assembly — refusing a block that cannot be proved complete
// ---------------------------------------------------------------------------
//
// Everything above asks whether a payload decodes to the right bytes. This section asks the harder
// question: given a whole console stream, does the host know when it is *missing* one? A CRC answers
// integrity and never absence — `frame.rs` documents that a record whose leading `~` vanished raises
// no integrity failure at all — so completeness here is proved by sequence: `n` says how many chunks
// were owed, and any index that never arrived refuses the block.
//
// The properties below are adversarial by construction. Each one damages a generated capture in the
// way a real fault does — a lost span of wire, samples that went wrong inside a frame that still
// validates, a retransmission carrying different bytes — and demands that the assembler both refuse
// and name the damage. Two of them run over generated input rather than a single hand-picked case:
// which record goes missing, and which byte gets corrupted, are chosen by `proptest`, because "we
// tested deleting a record" is weak evidence for "no record can go missing unnoticed".

use asperitas_logging::dump::{
    Abandoned, Action, Actions, BlockAssembler, Failure, Finish, Tally, CHUNK_RAW,
    MAX_CHUNKS_PER_BLOCK,
};
use asperitas_logging::frame::encode;

/// Chunks per generated block: three, so a block has a middle chunk to lose and a short final chunk
/// to exercise the length arithmetic in the same shape.
const ASSEMBLY_CHUNKS: u16 = 3;

/// Raw bytes in a generated block's final chunk. Not [`CHUNK_RAW`]: the grammar says the last chunk is
/// the short one, and the length checks only bite if that is true.
const ASSEMBLY_TAIL: usize = 40;

/// Raw bytes one generated block carries.
const ASSEMBLY_BLOCK_BYTES: usize = (ASSEMBLY_CHUNKS as usize - 1) * CHUNK_RAW + ASSEMBLY_TAIL;

/// Blocks per generated capture. Three is the smallest number where one block can die while another
/// survives on each side of it, which is the shape most assertions below depend on.
const ASSEMBLY_BLOCKS: u32 = 3;

/// Staging room for the widest block the grammar can name, so no test in this section is refused for
/// capacity unless it asks to be.
const ASSEMBLY_STAGING: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// Wire overhead of one record, shared with `tests/console_frame.rs`'s accounting law.
const FRAME_OVERHEAD: usize = PREFIX_LEN + TRAILER_LEN;

/// Deterministic samples for chunk `index` of `block`: different for every block and index, spanning
/// the whole byte range across a block so two payloads mixed together could not pass unnoticed.
fn samples(block: u32, index: u16) -> Vec<u8> {
    let len = if index + 1 == ASSEMBLY_CHUNKS {
        ASSEMBLY_TAIL
    } else {
        CHUNK_RAW
    };
    (0..len)
        .map(|byte| (((index as usize * 7 + byte) % 256) as u8) ^ block as u8)
        .collect()
}

/// One generated block, holding both views a test needs: the bodies an assembler consumes, and the
/// wire records a decoder receives.
struct TestBlock {
    /// Block id as it appears on the wire.
    id: u32,
    /// `AUDIO` bodies in send order.
    bodies: Vec<Vec<u8>>,
    /// The `AUDEND` body closing the block.
    summary_body: Vec<u8>,
    /// Complete wire records in send order, summary last.
    records: Vec<Vec<u8>>,
    /// Samples this block's summary checksums: the bytes a completed block must hand back.
    pcm: Vec<u8>,
}

/// Build one block, numbering its records from `seq` as a device would.
fn assembly_block(seq: &mut u32, id: u32) -> TestBlock {
    let mut pcm = Vec::with_capacity(ASSEMBLY_BLOCK_BYTES);
    let mut bodies = Vec::with_capacity(ASSEMBLY_CHUNKS as usize);
    let mut records = Vec::with_capacity(ASSEMBLY_CHUNKS as usize + 1);

    for index in 0..ASSEMBLY_CHUNKS {
        let raw = samples(id, index);
        pcm.extend_from_slice(&raw);
        bodies.push(audio_body_bytes(id, ASSEMBLY_CHUNKS, index, &raw));
        records.push(audio_wire(
            Level::Info,
            *seq,
            *seq * 4,
            id,
            ASSEMBLY_CHUNKS,
            index,
            &raw,
        ));
        *seq += 1;
    }

    let summary_body = audend_body_bytes(id, ASSEMBLY_CHUNKS, pcm.len() as u32, crc16_ccitt(&pcm));
    records.push(audend_wire(
        Level::Info,
        *seq,
        *seq * 4,
        id,
        ASSEMBLY_CHUNKS,
        pcm.len() as u32,
        crc16_ccitt(&pcm),
    ));
    *seq += 1;

    TestBlock {
        id,
        bodies,
        summary_body,
        records,
        pcm,
    }
}

/// A whole capture: blocks in send order, the concatenated stream, and the PCM the stream must yield.
struct Capture {
    /// Indexed by block id, which these helpers always assign as the position in send order.
    blocks: Vec<TestBlock>,
    stream: Vec<u8>,
    pcm: Vec<u8>,
}

fn capture() -> Capture {
    let mut seq = 1u32;
    let blocks: Vec<TestBlock> = (0..ASSEMBLY_BLOCKS)
        .map(|id| assembly_block(&mut seq, id))
        .collect();
    let stream: Vec<u8> = blocks
        .iter()
        .flat_map(|b| b.records.iter())
        .flatten()
        .copied()
        .collect();
    let pcm: Vec<u8> = blocks.iter().flat_map(|b| b.pcm.iter()).copied().collect();
    Capture {
        blocks,
        stream,
        pcm,
    }
}

impl Capture {
    /// Every block's id, in send order.
    fn ids(&self) -> Vec<u32> {
        self.blocks.iter().map(|b| b.id).collect()
    }

    /// Every block's id except `gone`.
    fn ids_except(&self, gone: usize) -> Vec<u32> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != gone)
            .map(|(_, b)| b.id)
            .collect()
    }

    /// Samples of every block except `gone`, in send order: the exact output a refusal must produce,
    /// which is how the tests prove a refused block contributes none of its chunks rather than
    /// contributing the ones that happened to arrive.
    fn pcm_without(&self, gone: usize) -> Vec<u8> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != gone)
            .flat_map(|(_, b)| &b.pcm)
            .copied()
            .collect()
    }

    /// This capture's wire bytes with one record removed. `record` indexes a block's records, where
    /// the last one is its summary.
    fn without_record(&self, block: usize, record: usize) -> Vec<u8> {
        let mut stream = Vec::with_capacity(self.stream.len());
        for (index, b) in self.blocks.iter().enumerate() {
            for (position, wire) in b.records.iter().enumerate() {
                if index == block && position == record {
                    continue;
                }
                stream.extend_from_slice(wire);
            }
        }
        stream
    }

    /// All records flattened, so a test can interleave other traffic between them.
    fn records_flat(&self) -> Vec<&[u8]> {
        self.blocks
            .iter()
            .flat_map(|b| &b.records)
            .map(Vec::as_slice)
            .collect()
    }
}

/// What one pass of decoder-plus-assembler produced, in the terms refusals get judged by.
#[derive(Debug, Default)]
struct Assembly {
    /// `(block, bytes)` for every block that completed, in completion order. Copied out immediately,
    /// because the assembler hands out one block at a time.
    completed: Vec<(u32, Vec<u8>)>,
    refused: Vec<(u32, Failure)>,
    abandoned: Vec<Abandoned>,
    /// Blocks a record arrived for after that block had already reached a verdict.
    late: Vec<u32>,
    malformed: usize,
    ignored: u64,
    tally: Tally,
    stats: Stats,
}

impl Assembly {
    /// Ids of every block that completed, in completion order.
    fn completed_ids(&self) -> Vec<u32> {
        self.completed.iter().map(|(id, _)| *id).collect()
    }

    /// Concatenation of every completed block's bytes, in completion order.
    fn pcm(&self) -> Vec<u8> {
        self.completed
            .iter()
            .flat_map(|(_, pcm)| pcm.iter())
            .copied()
            .collect()
    }

    /// Whether `block` completed.
    fn completed(&self, block: u32) -> bool {
        self.completed.iter().any(|(id, _)| *id == block)
    }

    /// The refusal naming `block`, if it was refused.
    fn failure_of(&self, block: u32) -> Option<Failure> {
        self.refused
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, reason)| *reason)
    }
}

/// Charge one record's events to the harness, copying out any samples the assembler just handed over.
///
/// One funnel for both drivers below, so a new [`Action`] variant cannot be quietly dropped by one of
/// them, and so the copy out of the staging buffer happens where the borrow rules make it obvious why
/// it is needed: the next record invalidates whatever the previous one pointed at.
fn absorb(assembler: &BlockAssembler, actions: &Actions, out: &mut Assembly) {
    for action in actions.as_slice() {
        match *action {
            Action::Complete { block, bytes } => {
                assert_eq!(
                    assembler.pcm().len(),
                    bytes,
                    "Complete promises exactly as many bytes as pcm() yields"
                );
                out.completed.push((block, assembler.pcm().to_vec()));
            }
            Action::Failed { block, reason } => out.refused.push((block, reason)),
            Action::Abandoned { block, missing } => {
                out.abandoned.push(Abandoned { block, missing })
            }
            Action::LateRecord { block, .. } => out.late.push(block),
            Action::Malformed { .. } => out.malformed += 1,
            Action::Ignored => out.ignored += 1,
            _ => {}
        }
    }
}

/// Run a stream through the real [`Decoder`] and a [`BlockAssembler`], offering `push_size` bytes at a
/// time, and check the accounting law at every push.
///
/// Chunk sizes are part of what a capture does to a reader: a law that balances only when the whole
/// stream arrives in one call is a law about the test harness.
fn assemble(bytes: &[u8], push_size: usize) -> Assembly {
    let mut staging = vec![0u8; ASSEMBLY_STAGING];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut decoder = Decoder::new();
    let mut out = Assembly::default();
    let mut pushed = 0u64;
    let mut framed = 0u64;

    let mut off = 0usize;
    while off < bytes.len() {
        let end = (off + push_size).min(bytes.len());
        let before = off;
        off += decoder.push(&bytes[off..end]);
        assert!(
            off > before,
            "decoder took nothing from offset {} of a {}-byte offer",
            before,
            bytes.len()
        );
        pushed += (off - before) as u64;
        drain(&mut decoder, &mut assembler, &mut out, &mut framed);
        check_law(pushed, framed, &decoder, "after push");
    }

    decoder.finish();
    drain(&mut decoder, &mut assembler, &mut out, &mut framed);
    check_law(pushed, framed, &decoder, "after finish");

    let Finish { abandoned, tally } = assembler.finish();
    if let Some(abandoned) = abandoned {
        out.abandoned.push(abandoned);
    }
    out.tally = tally;
    out.stats = decoder.stats();
    out
}

/// Hand every queued record to the assembler, counting what each one cost on the wire.
fn drain(
    decoder: &mut Decoder,
    assembler: &mut BlockAssembler,
    out: &mut Assembly,
    framed: &mut u64,
) {
    while let Some(record) = decoder.next_record() {
        *framed += (FRAME_OVERHEAD + record.body.len()) as u64;
        let actions = assembler.accept(record.body);
        absorb(assembler, &actions, out);
    }
}

/// `bytes_pushed == framed_bytes + discarded_bytes + buffered()` — the same law
/// `tests/console_frame.rs` enforces, restated here because each integration-test binary is its own
/// crate and cannot share a private helper.
fn check_law(pushed: u64, framed: u64, decoder: &Decoder, when: &str) {
    let discarded = decoder.stats().discarded_bytes;
    let buffered = decoder.buffered();
    assert!(
        buffered <= MAX_FRAME,
        "{when}: {} bytes buffered, more than one frame",
        buffered
    );
    let accounted = framed + discarded + buffered as u64;
    assert_eq!(
        pushed, accounted,
        "{when}: pushed {} but framed {} + discarded {} + buffered {} = {}",
        pushed, framed, discarded, buffered, accounted
    );
}

/// Feed bodies straight to the assembler — no framing, no decoder — for the cases where the grammar
/// and the geometry are under test rather than the wire.
fn feed(bodies: &[&[u8]], staging_len: usize) -> Assembly {
    let mut staging = vec![0u8; staging_len];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut out = Assembly::default();
    for body in bodies {
        let actions = assembler.accept(body);
        absorb(&assembler, &actions, &mut out);
    }
    let Finish { abandoned, tally } = assembler.finish();
    if let Some(abandoned) = abandoned {
        out.abandoned.push(abandoned);
    }
    out.tally = tally;
    out
}

/// A `STATUS` record: the chatter that shares the console with dump traffic and must flow past the
/// assembler without being mistaken for either loss or payload.
fn status_wire(seq: u32, now_ms: u32) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let encoded = encode(Level::Info, seq, now_ms, b"STATUS underruns=0", &mut buf);
    buf[..encoded.len].to_vec()
}

/// A permutation of `0..len`, for the tests that scramble arrival order.
fn permutation(len: usize) -> impl Strategy<Value = Vec<usize>> {
    Just((0..len).collect::<Vec<usize>>()).prop_shuffle()
}

/// The baseline the damaged cases are measured against: a healthy capture completes every block and
/// hands back exactly the samples that went in.
#[test]
fn clean_capture_completes_every_block() {
    let capture = capture();
    let assembly = assemble(&capture.stream, 4096);

    assert_eq!(assembly.completed_ids(), capture.ids());
    assert_eq!(assembly.pcm(), capture.pcm);
    assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
    assert!(assembly.abandoned.is_empty());
    assert!(assembly.late.is_empty());
    assert_eq!(assembly.malformed, 0);
    assert_eq!(assembly.stats.bad_frames, 0);
    assert_eq!(
        assembly.tally.chunks_stored,
        u64::from(ASSEMBLY_BLOCKS) * ASSEMBLY_CHUNKS as u64
    );
    assert_eq!(assembly.tally.pcm_bytes, capture.pcm.len() as u64);
}

proptest! {
    /// Any record can go missing, so *which* one is generated rather than chosen by whoever wrote the
    /// test. Deleting a chunk must refuse its block and name the absent index; deleting a summary must
    /// report the block abandoned, because nothing was missing — the verdict simply never arrived.
    /// Either way the refused block hands out no samples, and the blocks around it are untouched.
    #[test]
    fn deleting_any_record_is_detected(
        block in 0..ASSEMBLY_BLOCKS as usize,
        record in 0..ASSEMBLY_CHUNKS as usize + 1,
    ) {
        let capture = capture();
        let stream = capture.without_record(block, record);
        let assembly = assemble(&stream, 4096);
        let gone = block as u32;

        prop_assert_eq!(assembly.completed_ids(), capture.ids_except(block));
        prop_assert_eq!(assembly.pcm(), capture.pcm_without(block));

        if record < ASSEMBLY_CHUNKS as usize {
            // A chunk vanished: the summary arrives, counts what came, and refuses.
            let reason = assembly.failure_of(gone);
            match reason {
                Some(Failure::Missing(missing)) => {
                    prop_assert_eq!(missing.count(), 1, "one chunk missing: {:?}", missing);
                    prop_assert!(
                        missing.contains(record as u16),
                        "the missing list must name the deleted chunk: {:?}", missing
                    );
                }
                other => {
                    prop_assert!(false, "expected a missing-chunk refusal, got {:?}", other)
                }
            }
            prop_assert!(assembly.abandoned.is_empty());
        } else {
            // The summary vanished: every chunk is present, so there is nothing to name, and the
            // block dies when the next one's first chunk displaces it — or when the stream ends.
            prop_assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
            prop_assert_eq!(assembly.abandoned.len(), 1);
            prop_assert_eq!(assembly.abandoned[0].block, gone);
            prop_assert_eq!(
                assembly.abandoned[0].missing.count(),
                0,
                "no chunk was missing; only the verdict never came"
            );
        }
    }
}

proptest! {
    /// Corrupt one character inside a chunk's base64 payload region on the wire, leaving every other
    /// byte — including the record's own checksum — exactly as sent.
    ///
    /// Two receivers stand between that mutation and a false "complete": the frame CRC, which catches
    /// almost all of it, and behind that the strict base64 decode plus the block checksum, which catch
    /// what rides through inside a valid frame. What may not happen is silence. So the property is
    /// stated as a disjunction the generator cannot satisfy by luck: either some frame was rejected, or
    /// the block that owned the mutated byte did not complete.
    ///
    /// A 16-bit block checksum over ~4 kB of samples is thin cover on its own — SIGCOMM 2000's "When
    /// the CRC and TCP checksum disagree" measured real packets that passed end-to-end checks they
    /// should have failed — which is precisely why sequence and strict decoding carry the rest of this
    /// claim rather than the checksum alone.
    #[test]
    fn any_single_byte_body_mutation_is_caught(
        block in 0..ASSEMBLY_BLOCKS as usize,
        chunk in 0..ASSEMBLY_CHUNKS as usize,
        position in 0..CHUNK_RAW * 4 / 3,
        delta in 1u8..=255,
    ) {
        let capture = capture();
        let record = &capture.blocks[block].records[chunk];
        // Payload region: past the fixed-width header, up to the trailer's checksum digits.
        let start = dump::AUDIO_HEADER_LEN;
        let end = record.len() - TRAILER_LEN;
        let at = start + position % (end - start);

        let mut mutated = record.clone();
        mutated[at] ^= delta;
        let mut stream = Vec::with_capacity(capture.stream.len());
        for (index, b) in capture.blocks.iter().enumerate() {
            for (slot, wire) in b.records.iter().enumerate() {
                if index == block && slot == chunk {
                    stream.extend_from_slice(&mutated);
                } else {
                    stream.extend_from_slice(wire);
                }
            }
        }

        let assembly = assemble(&stream, 4096);
        prop_assert!(
            assembly.stats.bad_frames > 0 || !assembly.completed(block as u32),
            "silent success: a mutated payload byte at offset {} completed block {} (bad_frames={})",
            at,
            block,
            assembly.stats.bad_frames
        );
        // Whatever did complete carries exactly the bytes that were sent, never a plausible facsimile.
        for (id, pcm) in &assembly.completed {
            prop_assert_eq!(pcm, &capture.blocks[*id as usize].pcm);
        }
    }
}

/// The other half of the disjunction above, pinned rather than probabilistic: samples that go wrong
/// *before* framing ride through the frame CRC untouched, and only the block checksum stops them. This
/// is what a device fault looks like, as opposed to line noise.
#[test]
fn block_checksum_catches_damage_inside_valid_frames() {
    let mut bodies = Vec::new();
    let honest: Vec<u8> = (0..ASSEMBLY_CHUNKS)
        .flat_map(|index| samples(0, index))
        .collect();

    for index in 0..ASSEMBLY_CHUNKS {
        let mut raw = samples(0, index);
        if index == 0 {
            raw[0] ^= 0xff;
        }
        bodies.push(audio_body_bytes(0, ASSEMBLY_CHUNKS, index, &raw));
    }
    // The summary checksums the samples as they should have been, exactly as a writer would.
    bodies.push(audend_body_bytes(
        0,
        ASSEMBLY_CHUNKS,
        honest.len() as u32,
        crc16_ccitt(&honest),
    ));

    let assembly = feed(
        &bodies.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        ASSEMBLY_STAGING,
    );

    assert_eq!(assembly.completed_ids(), Vec::<u32>::new());
    match assembly.failure_of(0) {
        Some(Failure::Crc { shipped, computed }) => {
            assert_ne!(
                shipped, computed,
                "a refusal that agrees with itself proves nothing"
            );
        }
        other => panic!("expected a checksum refusal, got {other:?}"),
    }
    // Nothing was missing and no length disagreed: the checksum was the only thing left to object.
    assert_eq!(assembly.tally.malformed_records, 0);
}

proptest! {
    /// Chunks land at `chunk_index · CHUNK_RAW`, so permutation is structurally impossible rather than
    /// merely unlikely. Generated here: an independent arrival order per block, since a single lucky
    /// shuffle would prove nothing about the other two.
    #[test]
    fn arrival_order_changes_nothing(
        orders in prop::collection::vec(permutation(ASSEMBLY_CHUNKS as usize), ASSEMBLY_BLOCKS as usize),
    ) {
        let capture = capture();
        let mut stream = Vec::with_capacity(capture.stream.len());
        for (block, order) in capture.blocks.iter().zip(orders) {
            for index in order {
                stream.extend_from_slice(&block.records[index]);
            }
            // The summary still closes its own block: one producer writes sequentially, so interleaving
            // blocks would be loss rather than reordering (see `BlockAssembler`'s documentation).
            stream.extend_from_slice(&block.records[block.records.len() - 1]);
        }

        let assembly = assemble(&stream, 4096);
        prop_assert_eq!(assembly.completed_ids(), capture.ids());
        prop_assert_eq!(assembly.pcm(), capture.pcm.clone());
        prop_assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
    }
}

/// The blind spot `frame.rs` documents, made concrete: strip the leading `~` from every record and
/// there is nothing to decode, no integrity failure anywhere, and no honest report except "every byte
/// you gave me is gone". Zero completions, zero chunks, and the accounting law still balancing.
#[test]
fn stripped_start_markers_complete_nothing() {
    let capture = capture();
    let stripped: Vec<u8> = capture
        .stream
        .iter()
        .copied()
        .filter(|byte| *byte != b'~')
        .collect();
    let assembly = assemble(&stripped, 4096);

    assert!(assembly.completed.is_empty());
    assert_eq!(assembly.tally.chunks_stored, 0);
    assert_eq!(assembly.stats.records, 0);
    assert_eq!(
        assembly.stats.discarded_bytes,
        stripped.len() as u64,
        "bodies are sanitised, so nothing but a start marker can carry a tilde"
    );
    // And the experiment damaged what it claimed to: exactly one marker per record, which holds only
    // because bodies are sanitised on the way out and so no payload can carry a tilde.
    assert_eq!(
        capture.stream.iter().filter(|byte| **byte == b'~').count(),
        ASSEMBLY_BLOCKS as usize * (ASSEMBLY_CHUNKS as usize + 1),
        "start markers belong to records and nowhere else"
    );
}

/// A retransmission of identical bytes is the retry a lossy link expects: idempotent, and invisible in
/// the counters that gate CI. The same index carrying *different* bytes is two candidate shapes for one
/// block, so it gets a refusal naming the index instead.
#[test]
fn duplicates_are_idempotent_and_conflicts_are_refused() {
    let capture = capture();
    let block = &capture.blocks[0];
    let bodies: Vec<&[u8]> = block
        .bodies
        .iter()
        .chain(std::iter::once(&block.summary_body))
        .map(Vec::as_slice)
        .collect();

    // Identical re-send of the middle chunk.
    let mut retried = bodies.clone();
    retried.insert(2, block.bodies[1].as_slice());
    let assembly = feed(&retried, ASSEMBLY_STAGING);
    assert_eq!(assembly.completed_ids(), vec![0], "a retry changes nothing");
    assert_eq!(assembly.pcm(), block.pcm);
    assert_eq!(assembly.tally.duplicate_chunks, 1);
    assert_eq!(assembly.tally.conflicts, 0);
    assert!(assembly.refused.is_empty());

    // Same index, different samples, under a body that parses perfectly.
    let conflicting = audio_body_bytes(
        0,
        ASSEMBLY_CHUNKS,
        1,
        &samples(0, 1)[..]
            .iter()
            .map(|b| b ^ 0x5a)
            .collect::<Vec<_>>(),
    );
    let mut conflicted = bodies.clone();
    conflicted.insert(2, conflicting.as_slice());
    let assembly = feed(&conflicted, ASSEMBLY_STAGING);
    assert!(
        assembly.completed.is_empty(),
        "two candidate shapes complete as neither"
    );
    match assembly.failure_of(0) {
        Some(Failure::Conflict { index }) => assert_eq!(index, 1, "the refusal names the index"),
        other => panic!("expected a conflict refusal, got {other:?}"),
    }
    assert_eq!(assembly.tally.conflicts, 1);
}

/// Completion is final. A chunk arriving after its block's verdict is reported and dropped, because
/// reopening a published decision would mean a block could change shape depending on when a straggler
/// turned up.
#[test]
fn late_chunk_is_reported() {
    let capture = capture();
    let block = &capture.blocks[0];
    let mut bodies: Vec<&[u8]> = block
        .bodies
        .iter()
        .chain(std::iter::once(&block.summary_body))
        .map(Vec::as_slice)
        .collect();
    bodies.push(block.bodies[1].as_slice());

    let assembly = feed(&bodies, ASSEMBLY_STAGING);
    assert_eq!(assembly.completed_ids(), vec![0], "the verdict stands");
    assert_eq!(assembly.late, vec![0], "and the straggler is named");
    assert_eq!(assembly.tally.late_records, 1);
    assert!(assembly.refused.is_empty());
    assert_eq!(assembly.pcm(), block.pcm);
}

/// An `AUDEND` with no chunks behind it proves only what was owed: the missing list is the whole
/// block, from index zero to index `n − 1`.
#[test]
fn audend_alone_fails_with_full_missing_list() {
    let capture = capture();
    let summary = capture.blocks[2].summary_body.as_slice();
    let assembly = feed(&[summary], ASSEMBLY_STAGING);

    assert!(assembly.completed.is_empty());
    match assembly.failure_of(2) {
        Some(Failure::Missing(missing)) => {
            assert_eq!(missing.count(), ASSEMBLY_CHUNKS as usize);
            assert_eq!(missing.first(), Some(0));
            for index in 0..ASSEMBLY_CHUNKS {
                assert!(missing.contains(index), "index {index} should be missing");
            }
        }
        other => panic!("expected a missing-chunk refusal, got {other:?}"),
    }
}

/// Bodies that open with a dump verb but do not have its shape are refused where they broke, and
/// never stored. These are written as literals rather than built with `audio_body`, which rejects the
/// bad combinations outright — hand-writing them is what tests the receiving side's parser instead of
/// agreeing with our own writer.
#[test]
fn malformed_bodies_name_where_they_broke() {
    // Chunk index at or past `n`: a place in a block that says it has none.
    let beyond = b"AUDIO blk=0001 n=02 c=02 d=QUJD";
    // `n = 00`: no chunk zero to receive this.
    let empty_block = b"AUDIO blk=0001 n=00 c=00 d=QUJD";
    // Uppercase hex is not in the grammar; the encoder cannot produce it.
    let uppercase = b"AUDIO blk=000A n=02 c=00 d=QUJD";
    // A separator moved is a body whose skeleton no longer lines up.
    let shifted = b"AUDIO blk=0001,n=02 c=00 d=QUJD";
    // Payload characters beyond what the grammar budgets for one chunk.
    let oversized = format!(
        "AUDIO blk=0001 n=02 c=00 d={}",
        "A".repeat(dump::FULL_AUDIO_BODY_LEN - dump::AUDIO_HEADER_LEN + 4)
    )
    .into_bytes();

    for body in [
        &beyond[..],
        &empty_block[..],
        &uppercase[..],
        &shifted[..],
        &oversized[..],
    ] {
        let assembly = feed(&[body], ASSEMBLY_STAGING);
        assert_eq!(assembly.malformed, 1, "refused as malformed: {body:?}");
        assert_eq!(assembly.tally.chunks_stored, 0, "nothing stored: {body:?}");
        assert_eq!(
            assembly.tally.non_dump_records, 0,
            "not chatter either: {body:?}"
        );
    }

    // A malformed summary too, so the summary parser is covered by the same scepticism.
    let bad_summary = b"AUDEND blk=0001 n=03 bytes=00009 crc16=ZZZZ";
    let assembly = feed(&[&bad_summary[..]], ASSEMBLY_STAGING);
    assert_eq!(assembly.malformed, 1);
    assert!(
        assembly.refused.is_empty(),
        "a body nobody can read decides nothing"
    );
}

/// A staging buffer too small for a chunk at its geometric offset is reported as needing what the
/// geometry asks for — not silently truncated into a block that looks complete.
#[test]
fn buffer_too_small_for_n_reports_capacity() {
    let capture = capture();
    let block = &capture.blocks[1];
    let bodies: Vec<&[u8]> = block.bodies.iter().map(Vec::as_slice).collect();

    // Room for exactly one chunk: the second one belongs at `CHUNK_RAW` and will not fit.
    let assembly = feed(&bodies, CHUNK_RAW);
    assert!(assembly.completed.is_empty());
    assert_eq!(
        assembly.failure_of(1),
        Some(Failure::Capacity {
            needed: 2 * CHUNK_RAW
        }),
        "the refusal says how much room the block actually needs"
    );
}

/// One producer writes one block at a time, so a chunk naming a different block while one is open
/// means the open block's tail is gone. It is reported with what it still needed, and the newcomer
/// starts fresh — never held open alongside, which would turn a certain refusal into a guess.
#[test]
fn interleaved_blocks_abandon_the_open_one() {
    let capture = capture();
    let first = &capture.blocks[0];
    let second = &capture.blocks[1];

    let bodies: Vec<&[u8]> = first.bodies[..2]
        .iter()
        .chain(second.bodies.iter())
        .chain(std::iter::once(&second.summary_body))
        .map(Vec::as_slice)
        .collect();
    let assembly = feed(&bodies, ASSEMBLY_STAGING);

    assert_eq!(assembly.abandoned.len(), 1);
    assert_eq!(assembly.abandoned[0].block, first.id);
    assert_eq!(
        assembly.abandoned[0].missing.count(),
        1,
        "only the chunk that never got its turn is missing"
    );
    assert!(
        assembly.abandoned[0].missing.contains(2),
        "{:?}",
        assembly.abandoned[0].missing
    );
    assert_eq!(
        assembly.completed_ids(),
        vec![second.id],
        "the newcomer completes on its own"
    );
    assert_eq!(assembly.pcm(), second.pcm);
}

/// A stream ending mid-block leaves a block that may simply not have finished sending — and nothing
/// can prove that from an absent tail, so it is refused as the loss it might be.
#[test]
fn truncation_at_the_end_of_a_stream_is_refused() {
    let capture = capture();
    let owed: usize = capture.blocks[2].records[1..].iter().map(Vec::len).sum();
    let cut = capture.stream.len() - owed;
    let assembly = assemble(&capture.stream[..cut], 4096);

    assert_eq!(assembly.completed_ids(), capture.ids_except(2));
    assert_eq!(assembly.abandoned.len(), 1);
    assert_eq!(assembly.abandoned[0].block, 2);
    assert_eq!(assembly.abandoned[0].missing.count(), 2);
    assert_eq!(assembly.pcm(), capture.pcm_without(2));
}

proptest! {
    /// Dump traffic does not own the console: boot banners, `STATUS` counters, and log lines share the
    /// stream, and the assembler must let them through without charging them as loss or mistaking them
    /// for payload. Generated over how often chatter appears and how few bytes arrive per read, with
    /// the accounting law checked at every push inside [`assemble`] — the same law
    /// `tests/console_frame.rs` enforces, now holding over dump traffic too.
    #[test]
    fn accounting_law_holds_over_dump_traffic(
        push_size in 1usize..=700,
        status_every in 1usize..=4,
    ) {
        let capture = capture();
        let records = capture.records_flat();
        let mut stream = Vec::new();
        let mut seq = 1000u32;
        let mut inserted = 0u64;
        for (index, record) in records.iter().enumerate() {
            stream.extend_from_slice(record);
            if (index + 1) % status_every == 0 {
                stream.extend_from_slice(&status_wire(seq, index as u32));
                seq += 1;
                inserted += 1;
            }
        }

        let assembly = assemble(&stream, push_size);
        prop_assert_eq!(assembly.completed_ids(), capture.ids());
        prop_assert_eq!(assembly.pcm(), capture.pcm.clone());
        prop_assert_eq!(assembly.stats.bad_frames, 0);
        prop_assert_eq!(assembly.tally.non_dump_records, inserted);
        prop_assert_eq!(assembly.tally.records, (records.len() + inserted as usize) as u64);
    }
}

// ---------------------------------------------------------------------------
// Pipe headroom — a dump may never take the last MAX_FRAME bytes
// ---------------------------------------------------------------------------

use asperitas_logging::frame::{write_whole, WriteOutcome};
use asperitas_logging::LOG_PIPE_SIZE;

/// The device's ring, referenced rather than restated: `LOG_PIPE_SIZE` is ungated precisely so
/// this suite sweeps the capacity the firmware builds, and cannot quietly disagree with it.
const RING: usize = LOG_PIPE_SIZE;

/// A ring in the shape the device uses, held locally rather than in a `static`.
///
/// `NoopRawMutex` because no `critical-section` implementation is registered for host, which makes
/// `CriticalSectionRawMutex` an undefined symbol at *link* time — and `NoopRawMutex` is `!Sync`
/// (`PhantomData<*mut ()>`), which rules out the static form here. On target the static works,
/// because `CriticalSectionRawMutex` is `Sync`. Same stand-in `src/frame.rs`'s own suite uses.
type Ring = embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, RING>;

fn ring() -> Ring {
    embassy_sync::pipe::Pipe::new()
}

/// Commit through the real ring exactly as `try_emit_dump` will: `|c| LOG_PIPE.try_write(c).ok()`.
///
/// Deliberately *not* gated on [`dump::dump_fits`] — these tests are about whether the predicate's
/// verdict matches what the ring does, and asking the predicate first would assume the answer.
fn commit_frame(pipe: &Ring, frame: &[u8]) -> bool {
    matches!(
        write_whole(frame, pipe.free_capacity(), |chunk| {
            pipe.try_write(chunk).ok()
        }),
        WriteOutcome::Committed
    )
}

/// Advance both cursors `at` bytes into the backing array, leaving the ring empty by occupancy —
/// the state in which a wrap short-write happens.
fn park_cursors_at(pipe: &Ring, at: usize) {
    let filler = vec![b'f'; at];
    assert_eq!(pipe.try_write(&filler).ok(), Some(at));
    let mut drained = vec![0u8; at];
    assert_eq!(pipe.try_read(&mut drained).ok(), Some(at));
    assert_eq!(
        pipe.free_capacity(),
        RING,
        "ring must report itself empty before occupancy is set"
    );
}

/// Occupy `n` bytes, crossing the array end if it has to.
///
/// Reaching an arbitrary occupancy from parked cursors needs a loop for the same reason
/// `write_whole` exists: `try_write` returns only the contiguous run to the end of the backing
/// array even though `free_capacity()` reports total free.
fn fill_ring(pipe: &Ring, n: usize) {
    const FILLER: [u8; RING] = [b'x'; RING];
    let mut written = 0;
    while written < n {
        let chunk = pipe.try_write(&FILLER[written..n]).ok().unwrap_or(0);
        assert_ne!(
            chunk, 0,
            "ring stopped accepting filler at {written} of {n} bytes"
        );
        written += chunk;
    }
}

/// Everything currently in the ring, however it comes out.
fn drain_all(pipe: &Ring) -> Vec<u8> {
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while let Ok(n) = pipe.try_read(&mut buf) {
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    got
}

/// A real framed record whose body is exactly `body_len` bytes, built by the encoder rather than
/// assembled by hand, so the bytes under test are the bytes the device ships.
fn frame_of_body(body_len: usize) -> ([u8; MAX_FRAME], usize) {
    let mut out = [0u8; MAX_FRAME];
    let body = vec![b'a'; body_len];
    let enc = encode(Level::Info, 0x0bad_1dea, 4242, &body, &mut out);
    assert_eq!(enc.len, PREFIX_LEN + body_len + TRAILER_LEN);
    assert!(
        !enc.truncated,
        "a body of {body_len} <= MAX_BODY cannot come back truncated"
    );
    (out, enc.len)
}

/// Deterministic RNG so the randomized rounds replay identically on every run.
///
/// Step constants copied from `src/frame.rs`'s `XorShift` (a Numerical Recipes LCG) so results are
/// comparable between the two suites.
struct Lcg(u32);

impl Lcg {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.0 >> 16) as usize
    }
}

/// Every starting occupancy, crossed with the body lengths where a boundary could hide.
///
/// What CI can reach, stated plainly: the predicate and the ring. `try_emit_dump` itself is behind
/// `log-usb`, and on host that feature fails at *link* time for want of a `critical-section`
/// implementation — so nothing here fakes the device entry point. What this proves is the decision
/// the entry point makes, against a real `embassy_sync` ring at every capacity the device ring can
/// report, including across the array-end wrap where `try_write` short-writes.
#[test]
fn predicate_agrees_with_the_ring_at_every_occupancy() {
    // Cursor stops chosen to put a frame across the array end at many occupancies: 0 is the
    // pristine ring, 1 the cheapest possible offset, RING - 1 guarantees the next byte wraps.
    const CURSOR_STOPS: [usize; 3] = [0, 1, RING - 1];
    // Empty, one byte past empty, the full `AUDIO` chunk body, and `MAX_BODY` itself — the last two
    // straddle the point where a full frame plus the reserve exceeds the ring.
    const SWEPT_BODIES: [usize; 4] = [0, 1, dump::FULL_AUDIO_BODY_LEN, MAX_BODY];

    for &cursor_stop in &CURSOR_STOPS {
        for &body_len in &SWEPT_BODIES {
            for occupancy in 0..=RING {
                let pipe = ring();
                if cursor_stop > 0 {
                    park_cursors_at(&pipe, cursor_stop);
                }
                fill_ring(&pipe, occupancy);

                let free = pipe.free_capacity();
                assert_eq!(
                    free,
                    RING - occupancy,
                    "occupancy {occupancy} at cursor stop {cursor_stop} miscounted itself"
                );

                let (buf, len) = frame_of_body(body_len);
                let frame = &buf[..len];
                let admitted = dump::dump_fits(body_len, free);
                let accepted = commit_frame(&pipe, frame);

                if admitted {
                    assert!(
                        accepted,
                        "predicate admitted a {len}-byte frame at {free} free \
                         (body {body_len}, cursor stop {cursor_stop}) and the ring refused it"
                    );
                    assert!(
                        pipe.free_capacity() >= dump::RESERVE,
                        "an admitted commit left {} free, under the {}-byte reserve \
                         (body {body_len}, cursor stop {cursor_stop})",
                        pipe.free_capacity(),
                        dump::RESERVE,
                    );
                } else {
                    assert!(
                        !(len <= free && free - len >= dump::RESERVE),
                        "predicate refused a {len}-byte frame at {free} free that would have left \
                         the reserve intact (body {body_len}, cursor stop {cursor_stop})"
                    );
                }

                // Whatever landed must be whole and in order: the filler that was there first, then
                // the frame, byte for byte. A refusal must leave the filler alone.
                let mut expected = vec![b'x'; occupancy];
                if accepted {
                    expected.extend_from_slice(frame);
                }
                assert_eq!(
                    drain_all(&pipe),
                    expected,
                    "ring contents disagree with the accept/refuse verdict \
                     (body {body_len}, occupancy {occupancy}, cursor stop {cursor_stop}, \
                     admitted {admitted}, accepted {accepted})"
                );
            }
        }
    }
}

/// The post-condition as pure arithmetic, over every capacity the ring can report and every body
/// length the codec can frame. This is AC #5's claim stated as a sweep rather than as a type.
#[test]
fn a_dump_can_never_take_the_last_max_frame() {
    for free in 0..=RING {
        for body_len in 0..=MAX_BODY {
            let frame_len = PREFIX_LEN + body_len + TRAILER_LEN;

            if dump::dump_fits(body_len, free) {
                assert!(
                    free >= frame_len + dump::RESERVE && free - frame_len >= dump::RESERVE,
                    "admitted body {body_len} at {free} free: a {frame_len}-byte frame leaves {}",
                    free - frame_len,
                );
                // Monotone in capacity: a ring can only get emptier while a dumper waits, so a
                // retry never becomes *less* able. That is what lets TASK-038.03 back off on a
                // timer instead of reasoning about orderings.
                if free < RING {
                    assert!(
                        dump::dump_fits(body_len, free + 1),
                        "refusal is not monotone: body {body_len} fits at {free} but not at {}",
                        free + 1,
                    );
                }
            } else {
                // Every refusal has an arithmetic reason. `saturating_sub` is there to keep the
                // function total below the reserve, not to hide a case worth finding later.
                assert!(
                    frame_len + dump::RESERVE > free,
                    "refused body {body_len} at {free} free although a {frame_len}-byte frame \
                     plus the reserve fit",
                );
            }
        }
    }

    // Inside the reserve there is nothing to give, so even the smallest frame the codec can build
    // waits. The first capacity that admits it is exactly `RESERVE` plus that frame.
    assert!(
        !dump::dump_fits(0, dump::RESERVE),
        "the reserve must survive intact even for a zero-length body"
    );
    assert!(dump::dump_fits(0, dump::RESERVE + PREFIX_LEN + TRAILER_LEN));
}

/// The claim TASK-038.03's bench criterion rests on, expressed as something CI can check: with a
/// draining consumer, a maximum-size log record is never refused while at least [`MAX_FRAME`] bytes
/// were free — so a drop counter read during a dump cannot be blamed on the dump.
#[test]
fn log_records_survive_a_saturated_dump() {
    const ROUNDS: usize = 20_000;
    /// Bytes one drain-task wakeup takes out of the ring — `src/usb.rs`'s `DRAIN_BUF_SIZE`, so the
    /// consumer here drains like the one on the device (several 64-byte endpoint packets, not one).
    const DRAIN_BUF_SIZE: usize = 256;

    // The largest record the codec can build: this is what the reservation pays for.
    let (log_buf, log_len) = frame_of_body(MAX_BODY);
    assert_eq!(
        log_len, MAX_FRAME,
        "the log record under test must be a maximum-size one"
    );
    let log_frame = &log_buf[..log_len];

    // One full `AUDIO` chunk, framed by the same builder the device will call.
    let raw: Vec<u8> = (0..dump::CHUNK_RAW).map(|i| (i % 251) as u8).collect();
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audio_record(Level::Info, 0, 0, 0, 1, 0, &raw, &mut body, &mut frame_buf)
        .expect("a full chunk is a legal record");
    assert_eq!(enc.len, dump::FULL_AUDIO_FRAME_LEN);
    let dump_frame = frame_buf[..enc.len].to_vec();
    let dump_body_len = dump::FULL_AUDIO_BODY_LEN;

    let pipe = ring();
    let mut rng = Lcg::new(0x1234_5678);
    let mut expected: Vec<u8> = Vec::new();
    let mut got: Vec<u8> = Vec::new();
    let mut buf = [0u8; DRAIN_BUF_SIZE];
    let (mut log_commits, mut log_refusals) = (0usize, 0usize);
    let (mut dump_commits, mut dump_refusals) = (0usize, 0usize);

    for round in 0..ROUNDS {
        // Alternate which producer gets the earlier slot, so neither one is structurally favoured
        // by the loop's own order.
        let order = [true, false];
        let turns = if rng.next().is_multiple_of(2) {
            order
        } else {
            [false, true]
        };

        for is_dump_turn in turns {
            let free_before = pipe.free_capacity();
            if is_dump_turn {
                if dump::dump_fits(dump_body_len, free_before) {
                    assert!(
                        commit_frame(&pipe, &dump_frame),
                        "round {round}: the ring refused a dump frame the predicate admitted at {free_before} free"
                    );
                    assert!(
                        pipe.free_capacity() >= dump::RESERVE,
                        "round {round}: a committed dump left {} free, under the reserve",
                        pipe.free_capacity(),
                    );
                    expected.extend_from_slice(&dump_frame);
                    dump_commits += 1;
                } else {
                    dump_refusals += 1;
                }
            } else if commit_frame(&pipe, log_frame) {
                expected.extend_from_slice(log_frame);
                log_commits += 1;
            } else {
                log_refusals += 1;
                assert!(
                    free_before < MAX_FRAME,
                    "round {round}: a maximum-size log record was refused with {free_before} free \
                     — the dump took bytes reserved for it"
                );
            }
        }

        // Consumer: drain a random amount up to one packet, like the USB task does.
        let want = 1 + rng.next() % DRAIN_BUF_SIZE;
        if let Ok(n) = pipe.try_read(&mut buf[..want]) {
            got.extend_from_slice(&buf[..n]);
        }
    }

    while let Ok(n) = pipe.try_read(&mut buf) {
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }

    assert_eq!(
        expected, got,
        "stream is not the exact concatenation of what was committed \
         (log commits={log_commits} refusals={log_refusals}, \
         dump commits={dump_commits} refusals={dump_refusals})"
    );
    assert!(
        log_commits > 0 && dump_commits > 0 && dump_refusals > 0,
        "test exercised fewer than all three interesting paths: \
         log commits={log_commits} dump commits={dump_commits} dump refusals={dump_refusals}"
    );
}

/// The reserve must not be a deadlock dressed as a safety property: the moment the drainer catches
/// up, a dump is admitted again — even with the cursors parked where the next byte wraps.
#[test]
fn empty_ring_always_admits_a_dump() {
    assert!(
        dump::dump_fits(dump::FULL_AUDIO_BODY_LEN, RING),
        "an empty ring must admit a full dump chunk, or a dump would starve forever"
    );
    assert!(
        dump::dump_fits(MAX_BODY, RING),
        "an empty ring must admit even a maximum-body record"
    );

    for &cursor_stop in &[0usize, 1, RING - 1] {
        let pipe = ring();
        if cursor_stop > 0 {
            park_cursors_at(&pipe, cursor_stop);
        }

        let (buf, len) = frame_of_body(dump::FULL_AUDIO_BODY_LEN);
        let frame = &buf[..len];
        assert!(
            commit_frame(&pipe, frame),
            "empty ring at cursor stop {cursor_stop} refused a frame the predicate admitted"
        );
        assert!(
            pipe.free_capacity() >= dump::RESERVE,
            "cursor stop {cursor_stop}: first commit ate into the reserve"
        );

        // Draining frees the ring, and the very next dump goes in: progress is never blocked by the
        // reservation once the consumer has caught up.
        drain_all(&pipe);
        assert!(
            dump::dump_fits(dump::FULL_AUDIO_BODY_LEN, pipe.free_capacity()),
            "cursor stop {cursor_stop}: ring stayed unable to take a dump after full drain"
        );
        assert!(
            commit_frame(&pipe, frame),
            "cursor stop {cursor_stop}: second commit into a drained ring was refused"
        );
    }
}
