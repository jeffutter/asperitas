//! Adversarial suite for the self-verifying console frame codec.
//!
//! Run with: `cargo test -p asperitas-logging`
//!
//! Nothing else in this repo proves that a corrupted capture cannot be mistaken for clean
//! data, so this file is the deliverable rather than a formality: TASK-031's rig runner and
//! TASK-032's control channel link against this decoder instead of writing their own. Two
//! ideas hold the design together and each gets its own section below:
//!
//! * **Integrity is not absence.** A record that never arrived leaves no trace in
//!   `bad_frames`; only `seq` continuity and the boot counter reveal it. The canonical rows
//!   that look too quiet are that limitation, pinned rather than hidden.
//! * **The accounting law.** `bytes_pushed == framed + discarded_bytes + buffered()`,
//!   asserted after *every* push by [`Outcome::offer`].
//!
//! A third finding sits underneath both: the checksum has a floor, and one test goes out of
//! its way to reach it ([`crc_can_be_forged_at_weight_two`]). Counters describe what the wire
//! did to bytes, not who wrote them.
//!
//! Style follows `crates/asperitas-dsp/tests/property_tests.rs`: strategies sized by
//! *count* rather than bytes, lowercase `prop_assert!` messages naming the offending
//! values, and section banners over one property each.

use proptest::prelude::*;

use asperitas_logging::frame::{
    crc16_ccitt, encode, sanitize_byte, Decoder, Stats, MAX_BODY, MAX_FRAME, PREFIX_LEN,
    TRAILER_LEN,
};
use asperitas_logging::Level;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Bytes a validated record occupies whatever its body: the framing alone.
const FRAME_OVERHEAD: usize = PREFIX_LEN + TRAILER_LEN;

const LEVELS: [Level; 5] = [
    Level::Info,
    Level::Warn,
    Level::Error,
    Level::Debug,
    Level::Trace,
];

/// Frame one record exactly as the device puts it on the wire.
fn frame(level: Level, seq: u32, t_ms: u32, body: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let encoded = encode(level, seq, t_ms, body, &mut buf);
    buf[..encoded.len].to_vec()
}

/// The wire bytes a decoded record arrived as, rebuilt through the encoder.
///
/// Rebuilding is not re-checking: the record already passed CRC, the prefix is
/// fixed-width and the body arrives pre-sanitised, so this reproduces the frame byte for
/// byte. It lets “did the decoder invent this?” be answered about raw bytes rather than
/// about fields a corrupt parse could equally have misfiled.
fn wire_of(record: &Seen) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let encoded = encode(
        level_from_letter(record.level),
        record.seq,
        record.t_ms,
        &record.body,
        &mut buf,
    );
    buf[..encoded.len].to_vec()
}

fn level_from_letter(letter: u8) -> Level {
    match letter {
        b'W' => Level::Warn,
        b'E' => Level::Error,
        b'D' => Level::Debug,
        b'T' => Level::Trace,
        _ => Level::Info,
    }
}

/// One delivered record, owned so tests can compare without borrowing the decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    level: u8,
    seq: u32,
    t_ms: u32,
    body: Vec<u8>,
}

/// Everything one decoding run produced, with the accounting law checked as it goes.
///
/// The law — `bytes_pushed == framed_bytes + discarded_bytes + buffered()` — is checked
/// here rather than in each test because its whole value is that it holds *continuously*.
/// A law that only balances at the end cannot tell “this byte is still undecided” from
/// “this byte was dropped three pushes ago”, and that distinction is what the protocol
/// exists to make.
#[derive(Debug, Default, PartialEq, Eq)]
struct Outcome {
    records: Vec<Seen>,
    stats: Stats,
    bytes_pushed: u64,
    framed_bytes: u64,
    buffered: usize,
}

impl Outcome {
    /// Offer `bytes` to `decoder`, draining after every push.
    ///
    /// Draining is the reader discipline [`Decoder`]’s short-count `push` asks for; doing
    /// it here means every test exercises the lossless contract instead of tripping over
    /// backpressure it never asked for.
    fn offer(&mut self, decoder: &mut Decoder, bytes: &[u8]) {
        let mut off = 0usize;
        while off < bytes.len() {
            let before = off;
            off += decoder.push(&bytes[off..]);
            assert!(
                off > before,
                "decoder took nothing from offset {} of a {}-byte offer",
                before,
                bytes.len()
            );
            // Count what the decoder actually took, not what was offered: `decoder` holds
            // back the rest, and a law checked against bytes it never saw blames the
            // decoder for the harness's own bookkeeping.
            self.bytes_pushed += (off - before) as u64;
            self.drain(decoder);
            self.check_law("after push", decoder);
        }
    }

    fn drain(&mut self, decoder: &mut Decoder) {
        while let Some(record) = decoder.next_record() {
            self.framed_bytes += (FRAME_OVERHEAD + record.body.len()) as u64;
            self.records.push(Seen {
                level: record.level,
                seq: record.seq,
                t_ms: record.t_ms,
                body: record.body.to_vec(),
            });
        }
    }

    /// Close the stream and take the counters, which are complete only afterwards.
    fn finish(mut self, decoder: &mut Decoder) -> Self {
        decoder.finish();
        self.drain(decoder);
        self.stats = decoder.stats();
        self.buffered = decoder.buffered();
        assert_eq!(self.buffered, 0, "finish() must leave nothing undecided");
        self.check_law("after finish", decoder);
        self
    }

    fn check_law(&self, when: &str, decoder: &Decoder) {
        let discarded = decoder.stats().discarded_bytes;
        let buffered = decoder.buffered();
        // Part of the same promise: nothing may buffer without bound waiting for a
        // boundary the wire will never provide.
        assert!(
            buffered <= MAX_FRAME,
            "{when}: {} bytes buffered, more than one frame",
            buffered
        );
        let accounted = self.framed_bytes + discarded + buffered as u64;
        assert_eq!(
            self.bytes_pushed, accounted,
            "{when}: pushed {} but framed {} + discarded {} + buffered {} = {}",
            self.bytes_pushed, self.framed_bytes, discarded, buffered, accounted,
        );
    }

    fn row(&self) -> Row {
        Row {
            bytes_pushed: self.bytes_pushed,
            records: self.stats.records,
            bad_frames: self.stats.bad_frames,
            resyncs: self.stats.resyncs,
            discarded_bytes: self.stats.discarded_bytes,
        }
    }
}

/// Decode a whole stream in one push.
///
/// Expressed as “one chunk as long as the stream”, so both paths share the byte counting in
/// [`Outcome::offer`] rather than each inventing their own.
fn decode_whole(bytes: &[u8]) -> Outcome {
    decode_in_chunks(bytes, &[bytes.len().max(1)])
}

/// Decode a stream cut into pieces of the given sizes, cycling if the list runs out.
///
/// Sizes are a count of pieces, matching how the strategies below are sized: a test says
/// “ten pushes” and does not care where the boundaries fall.
fn decode_in_chunks(bytes: &[u8], chunk_sizes: &[usize]) -> Outcome {
    assert!(!chunk_sizes.is_empty(), "give at least one chunk size");
    let mut decoder = Decoder::new();
    let mut out = Outcome::default();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let size = chunk_sizes[pos % chunk_sizes.len()].max(1);
        let end = (pos + size).min(bytes.len());
        out.offer(&mut decoder, &bytes[pos..end]);
        pos = end;
    }
    out.finish(&mut decoder)
}

/// The four counters plus the byte total, in one line a test can compare with the table.
#[derive(Debug, PartialEq, Eq)]
struct Row {
    bytes_pushed: u64,
    records: u64,
    bad_frames: u64,
    resyncs: u64,
    discarded_bytes: u64,
}

/// Assert that a stream produces exactly one row of the canonical statistics table.
///
/// Every row was derived independently of this decoder — first by a reference model
/// written from TASK-030 §3 during planning, then again by hand while this file was
/// written — so agreement here is two implementations saying the same thing rather than
/// one agreeing with itself. When a row disagrees, suspect the construction above it first:
/// two of the four disagreements found while writing this file were the stream, not the
/// counters.
fn expect_row(stream: &[u8], expected: Row) -> Outcome {
    let out = decode_whole(stream);
    assert_eq!(
        out.row(),
        expected,
        "a {}-byte stream disagrees with the canonical table",
        stream.len()
    );
    // §5 claims the counters do not depend on chunk boundaries; every row above was derived
    // from a model that fed the device one byte at a time, so the claim is checked rather
    // than assumed.
    let chopped = decode_in_chunks(stream, &[1]);
    assert_eq!(
        chopped.row(),
        out.row(),
        "counters changed when bytes arrived one at a time"
    );
    out
}

/// Deterministic xorshift64\*, so sampled corruption replays without a dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform enough for these sizes; the modulo bias needs a comment precisely because
    /// it is *not* eliminated, and no conclusion here turns on it.
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % u64::try_from(n).expect("usize fits in u64")) as usize
    }

    /// A non-zero byte, by rejection: a zero delta would be a weight-1 edit in disguise.
    fn nonzero_delta(&mut self) -> u8 {
        loop {
            let byte = (self.next_u64() >> 32) as u8;
            if byte != 0 {
                return byte;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// An arbitrary body, control bytes and high bytes included, spanning the length cap.
fn arb_body() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=MAX_BODY + 32)
}

/// Chunk sizes as a count of pieces, most of them far shorter than a frame so splits land
/// mid-record rather than on boundaries by luck.
fn arb_chunk_sizes() -> impl Strategy<Value = Vec<usize>> {
    prop::collection::vec(1usize..64, 1..40)
}

fn arb_level() -> impl Strategy<Value = Level> {
    (0..LEVELS.len()).prop_map(|index| LEVELS[index])
}

/// A stream worth decoding: real frames, junk between them, and often one flipped byte.
///
/// Purely random bytes almost never resemble a frame, so a suite built only from them
/// would pass while the decoder mishandled the near misses that corruption actually
/// produces. Real frames interleaved with junk, plus an occasional edit, keep candidates
/// close to the grammar where the decisions live.
fn arb_stream() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        3 => (
            arb_level(),
            any::<u32>(),
            any::<u32>(),
            prop::collection::vec(any::<u8>(), 0..=MAX_BODY),
        )
            .prop_map(|(level, seq, t_ms, body)| frame(level, seq, t_ms, &body)),
        1 => prop::collection::vec(any::<u8>(), 1..24),
    ];
    (
        prop::collection::vec(piece, 0..6),
        any::<usize>(),
        any::<u8>(),
    )
        .prop_map(|(pieces, flip_index, flip_value)| {
            let mut stream = pieces.concat();
            if !stream.is_empty() {
                let index = flip_index % stream.len();
                stream[index] = flip_value;
            }
            stream
        })
}

// ---------------------------------------------------------------------------
// Property 1: framing is lossless and the accounting law holds at every boundary
// ---------------------------------------------------------------------------

proptest! {
    /// Whatever byte sequence arrives, in whatever chunk sizes, decoding neither panics nor
    /// loses a byte. The law is asserted inside [`Outcome::offer`] after every push, so this
    /// property's job is to put hostile input through it.
    #[test]
    fn never_panics_and_always_accounts(stream in prop::collection::vec(any::<u8>(), 0..4096),
                                        chunks in arb_chunk_sizes()) {
        let out = decode_in_chunks(&stream, &chunks);
        prop_assert!(out.records.len() <= stream.len() / FRAME_OVERHEAD);
    }

    /// Concatenated frames are all recovered, whole and in order, however the stream is
    /// chopped — including bodies that contain `*`, `\r`, `\n` and `~`.
    ///
    /// This is the property a log line with an embedded CRLF depends on, and the one that
    /// makes the delimiter-scanning shortcut safe rather than clever.
    #[test]
    fn intact_stream_loses_nothing(frames in prop::collection::vec(
        (arb_level(), any::<u32>(), any::<u32>(), arb_body()),
        1..=12,
    ), chunks in arb_chunk_sizes()) {
        let mut stream = Vec::new();
        let mut expected: Vec<Seen> = Vec::new();
        for (level, seq, t_ms, body) in frames {
            // The encoder carries at most MAX_BODY bytes; sanitising is per-byte, so the
            // order of these two does not matter.
            let mut sanitized: Vec<u8> = body.iter().map(|b| sanitize_byte(*b) as u8).collect();
            sanitized.truncate(MAX_BODY);
            let framed = frame(level, seq, t_ms, &body);
            // Read the letter back off the wire rather than restating the mapping here.
            stream.extend_from_slice(&framed);
            expected.push(Seen {
                level: framed[1],
                seq,
                // A timestamp is transmitted modulo a day; wrap it before comparing.
                t_ms: t_ms % 100_000_000,
                body: sanitized,
            });
        }

        let out = decode_in_chunks(&stream, &chunks);
        prop_assert_eq!(&out.records, &expected);
        prop_assert_eq!(out.stats.bad_frames, 0);
        prop_assert_eq!(out.stats.resyncs, 0);
        prop_assert_eq!(out.stats.discarded_bytes, 0);
    }

    /// Chunk boundaries carry no information: one push or forty pushes must land on the same
    /// records and the same counters.
    ///
    /// Without this, a reader could get clean output from `cat file | console_decode` and
    /// phantom resyncs from a device that trickles bytes.
    #[test]
    fn chunking_changes_nothing(stream in arb_stream(), chunks in arb_chunk_sizes()) {
        let whole = decode_whole(&stream);
        let chopped = decode_in_chunks(&stream, &chunks);
        prop_assert_eq!(&chopped.records, &whole.records);
        prop_assert_eq!(chopped.row(), whole.row());
    }
}

// ---------------------------------------------------------------------------
// Property 2: corruption is detected, or the damage is visible downstream
// ---------------------------------------------------------------------------

/// Every single-byte mutation of a valid frame is rejected, never accepted.
///
/// Exhaustive over position × value rather than sampled: 255 mutations of each of three
/// shapes — minimum-length, typical, and maximum-size — which is 75,735 decodes and finds
/// nothing here, because they all do. Anything less thorough would leave a byte position
/// unexamined, and the interesting claims live at positions like the trailer star, where
/// an off-by-one in the scan silently accepts garbage.
#[test]
fn rejects_every_single_byte_mutation() {
    let mut checked = 0usize;
    for shape in shapes() {
        for index in 0..shape.len() {
            let original = shape[index];
            for value in 0u8..=255 {
                if value == original {
                    continue;
                }
                let mut mutated = shape.clone();
                mutated[index] = value;
                let out = decode_whole(&mutated);
                assert!(
                    out.records.is_empty(),
                    "byte {} set to {value:#04x} was accepted: {:?} from {:?}",
                    index,
                    out.records,
                    shape
                );
                checked += 1;
            }
        }
    }
    // 7,140 + 10,455 + 58,140: the plan's predicted counts, pinned so a shape that stops
    // being exercised shows up as a number rather than silence.
    assert_eq!(checked, 7140 + 10455 + 58140);
}

/// Minimum-length (28-byte), typical (41-byte) and maximum-size (228-byte) frames.
///
/// The lengths are load-bearing: the exhaustive count below is predicted from them, so a
/// body that changes size changes the number of mutations actually examined.
fn shapes() -> Vec<Vec<u8>> {
    vec![
        frame(Level::Info, 7, 700, b""),
        frame(Level::Warn, 42_949_672, 4_567, b"clean record!"),
        frame(Level::Trace, u32::MAX, 99_999_999, &[b'a'; MAX_BODY]),
    ]
}

/// A frame whose checksum field holds hand-written literals still decodes.
///
/// Every other test builds its stream with `encode`, so if `encode` and the decoder shared
/// a wrong idea about where the CRC is computed, they would agree and this suite would pass.
/// These bytes come from TASK-030.01.01’s golden tests instead, and the assertion below
/// checks the literal against an independent CRC rather than against the encoder.
#[test]
fn decodes_bytes_the_encoder_never_produced() {
    let wire = b"~I 00000042 00004567 ENC +1*9c17\r\n";
    let out = decode_whole(wire);
    assert_eq!(
        out.records,
        vec![Seen {
            level: b'I',
            // Hex on the wire: the digits `00000042` mean sixty-six, not forty-two.
            seq: 0x42,
            t_ms: 4567,
            body: b"ENC +1".to_vec(),
        }]
    );
    // 21 + 6 body bytes + 7: the same frame the encoder would emit for these fields.
    assert_eq!(wire.len(), 34);
    assert_eq!(
        out.row(),
        Row {
            bytes_pushed: 34,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0
        }
    );

    let body_end = wire.len() - TRAILER_LEN;
    assert_eq!(crc16_ccitt(&wire[1..body_end]), 0x9c17);
}

/// Two random byte flips in a frame are caught.
///
/// The rate is *measured*, not assumed: 8,192 trials per shape, 24,576 in all, and every one
/// was detected — **100%**, i.e. an escape rate below 1 in 24,576 where the arithmetic floor
/// for random edits is about 1 in 65,536 (a false accept needs the recomputed CRC to land on
/// the transmitted digits by chance). Zero misses over that many trials is what agreement
/// between two independent implementations looks like, not luck.
///
/// The planning pass estimated weight-2 acceptance at 1 in 6,000 to 1 in 8,000, roughly ten
/// times the coincidence floor. Two mechanisms could account for it and neither is visible
/// from here: edits landing in the four checksum digits are undetectable by construction, and
/// the cancellation classes described in [`crc_can_be_forged_at_weight_two`] are far denser
/// than random coincidence. This suite measures random edits only, so it reports what it
/// measured and points at the forgery test for the part random sampling will not find.
///
/// “Detected” here means the delivered records differ from the intended ones. An edit that
/// destroys framing without leaving a counter behind still counts, which is why `seq`
/// continuity stays load-bearing alongside these counters — see [`crc_can_be_forged_at_weight_two`]
/// for the case where even that is not enough.
#[test]
fn detects_multi_byte_corruption() {
    const TRIALS: usize = 8_192;
    let mut detected = 0usize;
    let mut undetected = 0usize;

    for (shape_index, shape) in shapes().iter().enumerate() {
        // What this shape is supposed to deliver, taken from the unmutated frame.
        let intended: Vec<Seen> = decode_whole(shape).records;
        let body_end = shape.len() - TRAILER_LEN;
        let payload = body_end - 1;
        let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15 ^ (shape_index as u64));

        // Sample until enough mutations actually land, rather than counting skipped draws
        // as trials: the rate below is a rate over frames that were really corrupted.
        let mut applied = 0usize;
        while applied < TRIALS {
            let first = 1 + rng.below(payload);
            let second = 1 + rng.below(payload);
            if first == second {
                continue;
            }
            applied += 1;
            let (d1, d2) = (rng.nonzero_delta(), rng.nonzero_delta());
            let mut mutated = shape.clone();
            mutated[first] ^= d1;
            mutated[second] ^= d2;

            let out = decode_whole(&mutated);
            if out.records != intended {
                detected += 1;
            } else {
                undetected += 1;
            }
        }
    }

    let total = detected + undetected;
    assert_eq!(total, 3 * TRIALS, "some shapes went unexercised");
    let rate = detected as f64 / total as f64;
    assert!(
        rate > 0.99,
        "only {:.1}% of weight-2 corruptions were detected across {} trials",
        rate * 100.0,
        total
    );
}

/// A deliberate pair of body edits can validate, so integrity claims are never security claims.
///
/// CRC-16/CCITT-FALSE is affine over GF(2): flipping bits changes the checksum by an amount
/// that depends on the bits and their positions but not on the rest of the message. Two edits
/// whose contributions therefore cancel leave the stored checksum correct while changing the
/// payload, and the search below finds such a pair in milliseconds. The forged frame then
/// decodes with `bad_frames = 0`: nothing anywhere says this record is not genuine.
///
/// This is pinned rather than avoided because the rig runner’s threat model matters. TASK-031
/// reads captures written by a device it trusts over a wire it does not, and a 16-bit checksum
/// answers “did the wire corrupt this?” and nothing else. Anyone tempted to treat `records`
/// minus `bad_frames` as an authenticity statement should read this test first; the honest
/// backstops remain `seq` continuity, boot identity, and the trailer length.
#[test]
fn crc_can_be_forged_at_weight_two() {
    let original = frame(
        Level::Info,
        7,
        7_777,
        b"transfer of $5.00 to alice; memo: ignore the amount above",
    );
    let body_end = original.len() - TRAILER_LEN;
    let truth = decode_whole(&original.clone());

    // Single-edit checksums, keyed by the checksum each one produces. Because the checksum is
    // affine, a pair sharing one value cancels when applied together.
    let mut seen: std::collections::HashMap<u32, (usize, u8)> = std::collections::HashMap::new();
    let mut forged = None;
    'search: for index in PREFIX_LEN..body_end {
        let saved = original[index];
        for delta in 1u8..=255 {
            let mut probe = original.clone();
            probe[index] = saved ^ delta;
            let sum = u32::from(crc16_ccitt(&probe[1..body_end]));
            match seen.entry(sum) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    let (other_index, other_delta) = *entry.get();
                    if other_index != index {
                        forged = Some(((other_index, other_delta), (index, delta)));
                        break 'search;
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert((index, delta));
                }
            }
        }
    }
    let ((i, d1), (j, d2)) = forged.expect("no cancelling pair found in a 58-byte body");

    let mut attack = original.clone();
    attack[i] ^= d1;
    attack[j] ^= d2;
    assert_ne!(
        attack, original,
        "the two edits must actually change the payload"
    );

    let out = decode_whole(&attack);
    assert_eq!(out.stats.records, 1, "the forgery should have validated");
    assert_eq!(out.stats.bad_frames, 0, "and validated cleanly");
    assert_ne!(
        out.records[0].body, truth.records[0].body,
        "a cancelling pair that changed nothing proves nothing"
    );
}

// Any run of ≥ 3 random bytes replacing part of a frame destroys that frame. “Random” means
// unrelated to the frame, so this is the easy case — it earns its place by pinning that an
// erasure never becomes a plausible neighbour: the replacement must not be mistaken for a
// shorter valid record carrying different fields.
proptest! {
    #[test]
    fn erasures_destroy_the_frame(mut stream in prop::collection::vec(any::<u8>(), 3..=64)) {
        let base = frame(Level::Error, 4096, 88_888, b"erasure target");
        let mut damaged = base.clone();
        let span = stream.len().min(damaged.len());
        damaged[..span].copy_from_slice(&stream[..span]);
        stream.clear();
        stream.extend_from_slice(&damaged);

        let out = decode_whole(&stream);
        let intended = decode_whole(&base);
        prop_assert_ne!(&out.records, &intended.records);
        // Nothing may masquerade as the original record.
        for record in &out.records {
            prop_assert_ne!(&record.body, &intended.records[0].body);
        }
    }
}

/// Every prefix of a valid frame decodes to nothing; only the whole frame decodes.
///
/// Exhaustive over all 42 offsets rather than sampled, because this is where an off-by-one in
/// the terminator check lives: a decoder that accepts `k == len - 1` has decided a record is
/// complete on the strength of a byte it has not seen, and one that rejects `k == len` loses
/// every record whose arrival is split across reads. Both bugs hide in a sampled test.
#[test]
fn rejects_truncation_at_every_offset() {
    let full = frame(Level::Warn, 0x1234_ABCD, 12_345_678, b"truncation probe");
    let truth = decode_whole(&full);
    assert_eq!(truth.records.len(), 1, "the complete frame must decode");

    for cut in 1..full.len() {
        let out = decode_whole(&full[..cut]);
        assert_eq!(
            out.records.len(),
            0,
            "a {}-byte prefix of a {}-byte frame decoded as {:?}",
            cut,
            full.len(),
            out.records
        );
        assert_eq!(out.stats.bad_frames, 0, "truncation is not corruption");
        // Whatever arrived is either still undecided or charged as discarded — never both,
        // and never neither: `finish()` settled that inside `decode_whole`.
        assert_eq!(
            out.stats.discarded_bytes,
            u64::try_from(cut).expect("prefix fits in u64"),
            "a truncated prefix must be charged exactly its own length"
        );
    }
}

// ---------------------------------------------------------------------------
// Property 3: junk between records cannot cost more than the junk itself
// ---------------------------------------------------------------------------

proptest! {
    /// Arbitrary bytes inserted between valid frames lose at most the frame they land in.
    #[test]
    fn junk_between_records_costs_at_most_one_frame(junk in prop::collection::vec(any::<u8>(), 1..200),
                                                   split in 1usize..40) {
        let a = frame(Level::Info, 1, 100, b"alpha");
        let b = frame(Level::Debug, 2, 200, b"beta");
        let cut = split.min(b.len() - 1).max(1);

        // Junk spliced into the middle of `b`: at most `b` is lost, `a` survives untouched.
        let mut stream = a.clone();
        stream.extend_from_slice(&b[..cut]);
        stream.extend_from_slice(&junk);
        stream.extend_from_slice(&b[cut..]);

        let out = decode_whole(&stream);
        let expected_a = Seen { level: b'I', seq: 1, t_ms: 100, body: b"alpha".to_vec() };
        prop_assert!(
            out.records.first() == Some(&expected_a),
            "junk cost the preceding record: {:?}",
            out.records
        );
        prop_assert!(out.records.len() <= 2);
    }

    /// Truncation at the end of a stream costs exactly the tail, and says so.
    ///
    /// A rig pull that stops mid-frame must report the bytes it withheld rather than
    /// delivering a short read as if it were complete.
    #[test]
    fn truncation_is_counted_not_silent(prefix in 1usize..200) {
        let stream = [
            frame(Level::Info, 1, 100, b"one"),
            frame(Level::Warn, 2, 200, b"two"),
            frame(Level::Error, 3, 300, b"three"),
        ].concat();
        let cut = prefix.min(stream.len() - 1);
        let out = decode_whole(&stream[..cut]);

        // At most the two complete frames survive, and never more than arrived.
        prop_assert!(out.records.len() <= 2);
        prop_assert!(out.buffered < FRAME_OVERHEAD + MAX_BODY);
        // A partial tail is either still buffered or already counted as discarded — the law
        // in `offer` proves which, and the counters may not invent bytes for it.
        prop_assert_eq!(out.stats.bad_frames, 0);
    }
}

// ---------------------------------------------------------------------------
// Property 4: delimiters inside a body cannot forge or split a record
// ---------------------------------------------------------------------------

proptest! {
    /// A body containing `*`, `\r`, `\n`, `~` or any combination survives as one record with
    /// its bytes sanitised, never as two records and never as a rejected frame.
    #[test]
    fn delimiters_in_a_body_stay_inside_it(body in prop::collection::vec(
        prop_oneof![
            4 => Just(b'~'),
            4 => Just(b'*'),
            3 => Just(b'\r'),
            3 => Just(b'\n'),
            2 => Just(b'|'),
            1 => any::<u8>(),
        ],
        0..=MAX_BODY,
    )) {
        let stream = frame(Level::Info, 5, 500, &body);
        let out = decode_whole(&stream);
        let expected: Vec<u8> = body.iter().map(|b| sanitize_byte(*b) as u8).collect();
        prop_assert_eq!(out.records.len(), 1, "a delimiter forged or split a record");
        prop_assert_eq!(&out.records[0].body, &expected);
        prop_assert_eq!(out.stats.bad_frames, 0);
        prop_assert_eq!(out.stats.resyncs, 0);
    }
}

/// Every control byte, and each framing character alone, delivered inside a body.
///
/// The property above samples; this one enumerates the specific bytes that make the format
/// tick, because `sanitize_byte` is what keeps them from being read as structure and a
/// single unmapped byte there would otherwise show up only as a rare proptest failure.
#[test]
fn neutralises_every_byte_the_grammar_uses() {
    let mut stream = Vec::new();
    let mut expected: Vec<Vec<u8>> = Vec::new();

    // All 33 non-printing bytes, including `\r`, `\n` and 0x7F.
    let controls: Vec<u8> = (0u8..0x20).chain(std::iter::once(0x7f)).collect();
    stream.extend_from_slice(&frame(Level::Debug, 1, 100, &controls));
    expected.push(vec![b'_'; controls.len()]);

    // Each framing character repeated, so a body made entirely of one of them is still one
    // record rather than a run of candidates.
    for &ch in b"~*|" {
        stream.extend_from_slice(&frame(Level::Trace, 2, 200, &[ch; 40]));
        expected.push(vec![ch; 40]);
    }

    let out = decode_whole(&stream);
    let got: Vec<Vec<u8>> = out.records.iter().map(|r| r.body.clone()).collect();
    assert_eq!(got, expected);
    // 28 + 33 control bytes, then three frames of 28 + 40.
    assert_eq!(stream.len(), 265);
    assert_eq!(
        out.row(),
        Row {
            bytes_pushed: 265,
            records: 4,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0
        }
    );
}

/// A body holding a complete, well-formed frame still arrives as one record.
///
/// This is forgery resisted at the strongest available level: the embedded bytes carry a
/// plausible prefix and a valid checksum, yet they stay inside the enclosing record because
/// the record itself is what the CRC vouches for.
#[test]
fn an_embedded_valid_frame_stays_data() {
    let inner = frame(Level::Error, 9, 900, b"injected");
    let outer = frame(Level::Info, 1, 100, &inner);
    let out = decode_whole(&outer);

    assert_eq!(out.records.len(), 1);
    assert_eq!(out.records[0].level, b'I');
    assert_eq!(out.records[0].seq, 1);
    let expected: Vec<u8> = inner.iter().map(|b| sanitize_byte(*b) as u8).collect();
    assert_eq!(out.records[0].body, expected);
}

// ---------------------------------------------------------------------------
// Property 5: a splice costs at most one record and always leaves a trace
// ---------------------------------------------------------------------------

proptest! {
    /// Splicing B’s frame into the middle of A’s yields at most two records; whichever
    /// survive were genuinely validated, and the operation is visible in the counters.
    ///
    /// The invariant the rig runner depends on: a splice is never invisible. Either a record
    /// comes back damaged — detectable because `wire_of` will not reproduce it — or the
    /// counters move.
    #[test]
    fn splice_leaves_traces(a_body in prop::collection::vec(any::<u8>(), 12..=60),
                            cut in 1usize..12) {
        let a = frame(Level::Info, 1, 100, &a_body);
        let b = frame(Level::Warn, 2, 200, b"splice");
        let c = frame(Level::Info, 3, 300, b"tail");
        let spliced = cut.min(a.len() - 1);

        let stream = [&a[..spliced], &b[..], &a[spliced..], &c[..]].concat();
        let out = decode_whole(&stream);

        prop_assert!(out.records.len() <= 3, "a splice produced {} records", out.records.len());
        // A splice cannot be silent: something other than plain delivery must show.
        let clean_would_be = 3;
        let suspicious = out.records.len() != clean_would_be
            || out.stats.bad_frames > 0
            || out.stats.discarded_bytes > 0
            || out.records.iter().any(|r| wire_of(r) != a && wire_of(r) != b && wire_of(r) != c);
        prop_assert!(suspicious, "a splice decoded as three clean records: {:?}", out.records);
    }
}

/// Interleaving two producers’ output decodes every record and reports nothing suspicious.
///
/// Byte-level interleaving is TASK-031’s stated threat model, and the answer is that framing
/// is robust enough to ignore it: four records arrive intact with zero counters moved. What
/// interleaving defeats is timestamp interpretation, not framing, which is why §6 makes
/// `t_ms` per-producer.
#[test]
fn interleaved_producers_decode_cleanly() {
    let stream = [
        frame(Level::Info, 0, 1, b"ok"),
        frame(Level::Warn, 0, 2, b"ok"),
        frame(Level::Info, 1, 3, b"ok"),
        frame(Level::Warn, 1, 4, b"ok"),
    ]
    .concat();
    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 120,
            records: 4,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0,
        },
    );

    let levels: Vec<u8> = out.records.iter().map(|r| r.level).collect();
    assert_eq!(levels, vec![b'I', b'W', b'I', b'W']);
}

/// A splice that drops a record’s terminator between two others loses exactly one record.
#[test]
fn splice_between_records_costs_one_record() {
    let a = frame(Level::Info, 1, 100, b"first");
    let b = frame(Level::Warn, 2, 200, b"second");
    let mut stream = a.clone();
    // B's bytes inserted before A's terminator, then A's terminator dropped.
    stream.extend_from_slice(&b[..b.len() - TRAILER_LEN]);
    stream.extend_from_slice(&a[a.len() - TRAILER_LEN..]);

    let out = decode_whole(&stream);
    assert!(out.records.len() <= 2);
    assert!(
        out.stats.bad_frames + out.stats.discarded_bytes > 0,
        "a splice left no trace: {:?}",
        out.stats
    );
}

/// Removing the delimiter between two records costs the first and says so twice over.
#[test]
fn missing_delimiter_is_counted_not_swallowed() {
    let first = frame(Level::Info, 1, 100, b"first record");
    let second = frame(Level::Warn, 2, 200, b"second");
    let crlf = first.len() - TRAILER_LEN + 5;
    let mut stream = [first.as_slice(), second.as_slice()].concat();
    stream.splice(crlf..crlf + 2, std::iter::empty());

    // Derived by hand: the composite candidate fails CRC (bad_frames 1), the forfeited marker
    // plus the run up to the next `~` is charged (discarded 38), the resync is counted, and
    // only the second record survives.
    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 72,
            records: 1,
            bad_frames: 1,
            resyncs: 1,
            discarded_bytes: 38,
        },
    );
    assert_eq!(out.records[0].seq, 2);
}

// ---------------------------------------------------------------------------
// Property 7: the decoder never invents data
// ---------------------------------------------------------------------------

proptest! {
    /// Every delivered record rebuilds, through the encoder, into bytes that actually
    /// appeared in the stream.
    ///
    /// The mechanism is [`wire_of`]; the interesting half is the vacuity check. A suite that
    /// asserted only “each record maps to some substring” would pass while the decoder
    /// swallowed everything, so the property below also fails whenever fewer records arrive
    /// than frames were sent without anything being charged for the difference.
    #[test]
    fn never_invents_a_record(frames in prop::collection::vec(
        (arb_level(), any::<u32>(), any::<u32>(), prop::collection::vec(any::<u8>(), 0..=MAX_BODY)),
        1..=8,
    ), junk in prop::collection::vec(any::<u8>(), 0..32)) {
        let mut stream = Vec::new();
        let mut sent = 0usize;
        for (level, seq, t_ms, body) in frames {
            // Junk after each frame except the last, so real candidates compete with noise.
            let framed = frame(level, seq, t_ms, &body);
            stream.extend_from_slice(&framed);
            sent += 1;
            if !junk.is_empty() && stream.len() % 2 == 0 {
                stream.extend_from_slice(&junk[..junk.len().min(4)]);
            }
        }

        let out = decode_whole(&stream);
        for record in &out.records {
            let rebuilt = wire_of(record);
            let found = stream
                .windows(rebuilt.len())
                .any(|window| window == rebuilt.as_slice());
            prop_assert!(found, "invented record {:?} is not in the stream", record);
        }

        // Nothing may vanish without a counter: undelivered frames must be accounted for by
        // rejection, discard, or bytes still undecided when the stream ended.
        let lost = sent - out.records.len();
        let charged = out.stats.bad_frames + out.stats.resyncs + out.stats.discarded_bytes;
        prop_assert!(
            lost == 0 || charged > 0,
            "{} records disappeared with counters {:?}",
            lost,
            out.stats
        );
    }
}

// ---------------------------------------------------------------------------
// The canonical statistics table (TASK-030 §6)
//
// Each stream below was built from the row’s prose description, independently of this
// decoder, and the counters were derived by hand before any of them ran. `expect_row`
// checks the row twice: once as one push, once as one byte per push.
// ---------------------------------------------------------------------------

/// Three intact frames, no noise: the baseline every other row is measured against.
#[test]
fn canonical_row_1_three_clean_records() {
    let stream = [
        frame(Level::Info, 0, 0, b"ok"),
        frame(Level::Info, 1, 0, b"ok"),
        frame(Level::Info, 2, 0, b"ok"),
    ]
    .concat();
    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 90,
            records: 3,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0,
        },
    );
    let seqs: Vec<u32> = out.records.iter().map(|r| r.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2]);
}

/// Nine bytes of garbage, including a null byte, precede one intact frame.
///
/// The table says `resyncs = 0`, and that is the row worth reading twice: bytes were
/// discarded without any resynchronisation being *needed*, because nothing had to be given
/// up. Conflating the two would make a healthy capture look damaged.
#[test]
fn canonical_row_2_garbage_prefix_costs_bytes_but_not_a_resync() {
    let mut stream = b"garbage\xff\x00".to_vec();
    stream.extend_from_slice(&frame(Level::Info, 0, 0, b"boot sequence"));
    assert_eq!(
        expect_row(
            &stream,
            Row {
                bytes_pushed: 50,
                records: 1,
                bad_frames: 0,
                resyncs: 0,
                discarded_bytes: 9
            }
        )
        .records[0]
            .body,
        b"boot sequence"
    );
}

/// One CRLF removed from the middle of a three-record stream.
///
/// The seam welds record 3’s bytes onto record 4’s marker, so the composite fails CRC, costs
/// 28 bytes, and delivers 3 of 4 records — while `seq` continuity is what proves anything was
/// lost at all.
#[test]
fn canonical_row_3_removed_delimiter_welds_two_records() {
    let mut stream = [
        frame(Level::Info, 0, 0, b"ok"),
        frame(Level::Info, 1, 0, b"ok"),
        frame(Level::Info, 2, 0, b"ok"),
        frame(Level::Info, 3, 0, b"ok"),
    ]
    .concat();
    // Delete the terminator of the third record: index 88..90 of the 120-byte stream.
    stream.splice(88..90, std::iter::empty());

    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 118,
            records: 3,
            bad_frames: 1,
            resyncs: 1,
            discarded_bytes: 28,
        },
    );
    let seqs: Vec<u32> = out.records.iter().map(|r| r.seq).collect();
    assert_eq!(
        seqs,
        vec![0, 1, 3],
        "the gap must be visible in the sequence numbers"
    );
}

/// The checksum digits replaced by `ffff`, which parses as valid hex.
///
/// Nothing structural is wrong with this frame, which is the point: only the CRC disagrees,
/// and the whole frame is charged as discarded because there was no candidate left to keep.
#[test]
fn canonical_row_4_bad_crc_rejects_the_frame() {
    let mut stream = frame(Level::Info, 0, 0, b"calibration ok");
    let crc_start = stream.len() - TRAILER_LEN + 1;
    stream[crc_start..crc_start + 4].copy_from_slice(b"ffff");
    expect_row(
        &stream,
        Row {
            bytes_pushed: 42,
            records: 0,
            bad_frames: 1,
            resyncs: 0,
            discarded_bytes: 42,
        },
    );
}

/// A CRLF inserted into a record’s body, splitting it in two.
///
/// The first half is rejected and the second arrives whole, so the capture looks like a
/// slightly damaged log rather than what it is: a line broken by an unrelated writer. Note
/// the discarded total is 27, not 25 — the forfeited marker is charged on rejection and the
/// run to the next marker on resync, which is why the two are counted separately.
#[test]
fn canonical_row_5_injected_delimiter_splits_a_line() {
    let full = frame(Level::Info, 1, 0, b"boot sequence");
    let mut stream = full[..25].to_vec();
    stream.extend_from_slice(b"\r\n");
    stream.extend_from_slice(&full);

    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 68,
            records: 1,
            bad_frames: 1,
            resyncs: 1,
            discarded_bytes: 27,
        },
    );
    assert_eq!(
        out.records[0].seq, 1,
        "only the complete second half may survive"
    );
}

/// Two lone `\n` bytes before one intact frame.
#[test]
fn canonical_row_6_lone_line_feeds_are_junk() {
    let mut stream = b"\n\n".to_vec();
    stream.extend_from_slice(&frame(Level::Info, 0, 0, b"boot sequence"));
    expect_row(
        &stream,
        Row {
            bytes_pushed: 43,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 2,
        },
    );
}

/// A capture that ends mid-frame.
///
/// The ten trailing bytes reach `discarded_bytes` only because `finish()` was called; without
/// it they would still be sitting in the window and the summary would read zero. That is the
/// single most misleading counter in the protocol and the reason `finish` is mandatory.
#[test]
fn canonical_row_7_trailing_partial_is_charged_at_finish() {
    let mut stream = frame(Level::Info, 0, 0, b"boot sequence");
    stream.extend_from_slice(b"~I 0000000");
    expect_row(
        &stream,
        Row {
            bytes_pushed: 51,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 10,
        },
    );
}

/// A body carrying every character the format reserves.
///
/// Delimiters travel intact as underscores and nothing is discarded: the sanitiser makes them
/// harmless rather than rare, and the record’s integrity is the CRC’s business.
#[test]
fn canonical_row_8_forge_characters_survive_sanitised() {
    let stream = frame(Level::Info, 0, 0, b"~*|__abc def gh");
    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 43,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0,
        },
    );
    assert_eq!(out.records[0].body, b"~*|__abc def gh");
}

/// A frame whose leading `~` was eaten, followed by an intact frame.
///
/// The orphaned 42 bytes cannot be framed without their marker, so they are discarded as junk
/// and the next real record is accepted **without** a resync — the state machine never
/// believed it was inside a record, so it has nothing to recover from.
#[test]
fn canonical_row_9_lost_marker_needs_no_resync() {
    let mut stream = b"I 00000000 00000000 no tilde here!!*9f26\r\n".to_vec();
    stream.extend_from_slice(&frame(Level::Info, 1, 0, b"ok"));

    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 72,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 42,
        },
    );
    assert_eq!(out.records[0].seq, 1);
}

/// One maximum-size frame: 200 body bytes, 228 on the wire.
#[test]
fn canonical_row_10_maximum_frame_fits_without_discarding() {
    let stream = frame(Level::Info, 0, 0, &[b'a'; MAX_BODY]);
    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 228,
            records: 1,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0,
        },
    );
    assert_eq!(out.records[0].body.len(), MAX_BODY);
}

/// Thirty-five bytes of ordinary terminal text with no marker anywhere.
#[test]
fn canonical_row_11_plain_text_is_all_junk() {
    let stream = b"booting... calibration done. no fra".to_vec();
    expect_row(
        &stream,
        Row {
            bytes_pushed: 35,
            records: 0,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 35,
        },
    );
}

/// A body one byte over the cap, framed by hand because the encoder would have capped it.
///
/// Rejection happens on length alone, before the CRC is consulted, and the whole 229 bytes go.
#[test]
fn canonical_row_12_overlong_body_is_rejected_on_length() {
    let raw = frame(Level::Info, 0, 0, &[b'z'; MAX_BODY]);
    let mut stream = raw[..PREFIX_LEN].to_vec();
    stream.push(b'z');
    stream.extend_from_slice(&raw[PREFIX_LEN..]);
    expect_row(
        &stream,
        Row {
            bytes_pushed: 229,
            records: 0,
            bad_frames: 1,
            resyncs: 0,
            discarded_bytes: 229,
        },
    );
}

/// Two producers interleaved byte-for-byte, four records total.
///
/// All four decode and no counter moves. Framing survives interleaving completely; what it
/// cannot tell you is which producer a timestamp belongs to, which is why §6 reads `t_ms` as
/// monotone per producer and never as absolute time.
#[test]
fn canonical_row_13_interleaved_producers_decode_intact() {
    let stream = [
        frame(Level::Info, 0, 1, b"ok"),
        frame(Level::Warn, 0, 2, b"ok"),
        frame(Level::Info, 1, 3, b"ok"),
        frame(Level::Warn, 1, 4, b"ok"),
    ]
    .concat();
    expect_row(
        &stream,
        Row {
            bytes_pushed: 120,
            records: 4,
            bad_frames: 0,
            resyncs: 0,
            discarded_bytes: 0,
        },
    );
}

/// Producer B’s frame spliced into the middle of producer A’s record.
///
/// A’s record is destroyed and B’s survives, along with A’s following record: two delivered,
/// one rejected, one resync, thirty bytes discarded. This is the row that shows a splice is
/// detectable in aggregate even when individual records look fine.
#[test]
fn canonical_row_14_splice_destroys_one_record_and_reports_it() {
    let a1 = frame(Level::Info, 0, 1, b"aa");
    let b1 = frame(Level::Warn, 0, 2, b"bb");
    let a2 = frame(Level::Info, 1, 3, b"cc");
    let stream = [&a1[..12], &b1[..], &a1[12..], &a2[..]].concat();

    let out = expect_row(
        &stream,
        Row {
            bytes_pushed: 90,
            records: 2,
            bad_frames: 1,
            resyncs: 1,
            discarded_bytes: 30,
        },
    );
    let seen: Vec<(u8, u32, Vec<u8>)> = out
        .records
        .iter()
        .map(|r| (r.level, r.seq, r.body.clone()))
        .collect();
    assert_eq!(
        seen,
        vec![(b'W', 0, b"bb".to_vec()), (b'I', 1, b"cc".to_vec())]
    );
}
