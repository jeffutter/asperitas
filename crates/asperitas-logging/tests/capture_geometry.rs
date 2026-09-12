//! Derivation suite for the capture-ring geometry in `asperitas_logging::capture`.
//!
//! Run with: `cargo test -p asperitas-logging`
//!
//! The ring and the console stream describe the same capture, and until this file nothing proved it:
//! `dump` owned the wire grammar, the ring's shape lived in prose, and the pair happened to be
//! compatible by exactly zero margin — a 32 KiB block is 254 full chunks plus a 2-byte tail, i.e.
//! 255 chunks, which *is* the ceiling the 2-digit `n` field can name. Discovering that off-by-one at
//! runtime means `dump`'s body builder refuses mid-dump, at the bench, with ears on the line.
//!
//! Two ideas hold the design together, one section each below:
//!
//! * **The module cannot grade its own homework.** Every figure is pinned to a literal written once,
//!   here, and the load-bearing one — the exact wire cost of a block — is confirmed by an oracle that
//!   encodes a real 255-chunk block through `dump::audio_record` / `dump::audend_record` and sums
//!   what the codec actually produced. Comparing `capture`'s arithmetic against itself would prove
//!   nothing about the wire.
//! * **Tightness is pinned, not implied.** The two grammar gates differ: 127 spare bytes against
//!   `MAX_BLOCK_BYTES`, but *zero* spare chunks. A test pins each number so neither margin can be
//!   quietly spent.
//!
//! No proptest: this is exhaustive arithmetic over ~a dozen integers, and a random sampler can only
//! re-discover the single point it would assert on. The state machine gets the same treatment — all
//! sixteen pairs enumerated rather than sampled.
//!
//! Style follows `tests/console_frame.rs`: helper docs, section banners over one idea each, and
//! third-person indicative test names.

use asperitas_logging::capture::{self, BlockState};
use asperitas_logging::dump::{
    self, AUDIO_HEADER_LEN, CHUNK_RAW, FULL_AUDIO_FRAME_LEN, MAX_AUDEND_BODY_LEN, MAX_BLOCK_BYTES,
    MAX_CHUNKS_PER_BLOCK,
};
use asperitas_logging::frame::{
    crc16_ccitt, crc16_ccitt_update, CRC16_INITIAL, MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN,
};
use asperitas_logging::Level;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Shaped after `tests/console_dump.rs`'s `audio_wire` / `audend_wire`. Each tests/ file is its own
// crate and there is no shared helpers module here, so the ten lines are copied rather than a
// common module invented that no other file wants. They report the frame *length*, which is all
// this suite sums.
/// Wire length of one complete `AUDIO` record carrying `raw` as chunk `chunk_index` of `chunks`.
fn audio_frame_len(block_index: u32, chunks: u16, chunk_index: u16, raw: &[u8]) -> usize {
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
    assert!(
        !enc.truncated,
        "the body shipped shortened, so its length no longer describes the record"
    );
    enc.len
}

/// Wire length of the `AUDEND` that closes a block of `total_bytes` raw bytes.
fn audend_frame_len(block_index: u32, chunks: u16, total_bytes: u32, crc: u16) -> usize {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audend_record(
        Level::Info,
        0x42,
        4567,
        block_index,
        chunks,
        total_bytes,
        crc,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    assert!(
        !enc.truncated,
        "the body shipped shortened, so its length no longer describes the record"
    );
    enc.len
}

/// The four edges the hand-off allows, spelled out independently of [`capture::transition_ok`] so
/// the test is a witness rather than a mirror.
const LEGAL_EDGES: [(BlockState, BlockState); 4] = [
    (BlockState::Free, BlockState::Filling),
    (BlockState::Filling, BlockState::Full),
    (BlockState::Full, BlockState::Dumping),
    (BlockState::Dumping, BlockState::Free),
];

// ---------------------------------------------------------------------------
// Hardware inputs and ring size
// ---------------------------------------------------------------------------

/// One block is 512 callbacks of mono 16-bit at 48 kHz.
#[test]
fn a_block_is_512_callbacks_of_mono_16_bit() {
    assert_eq!(capture::SAMPLE_RATE_HZ, 48_000);
    assert_eq!(capture::CAPTURE_BYTES_PER_SAMPLE, 2);
    assert_eq!(capture::FRAMES_PER_CALLBACK, 32);
    assert_eq!(capture::CALLBACK_BYTES, 64);
    assert_eq!(capture::RING_BLOCK_BYTES, 32_768);
    assert_eq!(capture::callbacks_per_block(), 512);
    assert_eq!(capture::samples_per_block(), 16_384);
    assert_eq!(capture::BYTES_PER_SECOND, 96_000);
}

/// The ring holds 1 024 blocks — 32 MiB, 349.5 seconds of mono capture.
#[test]
fn the_ring_holds_349_point_5_seconds() {
    assert_eq!(capture::RING_BLOCKS, 1_024);
    assert_eq!(capture::RING_BYTES, 33_554_432);
    assert_eq!(capture::ring_seconds_floor(), 349);
    assert_eq!(capture::ring_duration_micros(), 349_525_333);

    // The durations are microseconds-exact, so the floored seconds must agree with them rather than
    // being derived along a second, wider path that could drift.
    assert_eq!(
        capture::ring_seconds_floor() as u64,
        capture::ring_duration_micros() / 1_000_000
    );
}

// ---------------------------------------------------------------------------
// Coupling to the wire grammar — where the margins live
// ---------------------------------------------------------------------------

/// A block needs 255 `AUDIO` chunks and therefore puts 256 records on the wire.
#[test]
fn a_block_needs_255_chunks_and_256_records() {
    assert_eq!(capture::full_chunks_per_block(), 254);
    assert_eq!(capture::tail_chunk_bytes(), 2);
    assert_eq!(capture::chunks_per_block(), 255);
    assert_eq!(capture::records_per_block(), 256);

    // Zero-margin gate, restated as a host assertion: the ring sits exactly on the chunk ceiling, so
    // any widening of either side breaks this line before it breaks a dump.
    assert_eq!(
        capture::chunks_per_block(),
        MAX_CHUNKS_PER_BLOCK,
        "the ring block's chunk count left the grammar's ceiling it was pinned to"
    );

    // The byte gate is tight but not at the ceiling: pin the slack too, so nobody reads gate 1 as
    // zero-margin and then spends it.
    assert_eq!(
        MAX_BLOCK_BYTES - capture::RING_BLOCK_BYTES,
        127,
        "documentation says a ring block leaves 127 bytes under the grammar's widest describable block"
    );
}

/// One block costs 57 788 wire bytes — recomputed here from the grammar's public constants.
#[test]
fn one_block_costs_57788_wire_bytes() {
    let tail_frame = PREFIX_LEN
        + AUDIO_HEADER_LEN
        + dump::encoded_len(capture::tail_chunk_bytes())
        + TRAILER_LEN;
    let audend_frame = PREFIX_LEN + MAX_AUDEND_BODY_LEN + TRAILER_LEN;
    let budget =
        capture::full_chunks_per_block() * FULL_AUDIO_FRAME_LEN + tail_frame + audend_frame;

    assert_eq!(tail_frame, 59, "a 2-byte tail chunk frames to 59 bytes");
    assert_eq!(audend_frame, 71, "the closing AUDEND frames to 71 bytes");
    assert_eq!(budget, 57_788);
    assert_eq!(
        capture::wire_bytes_per_block(),
        budget,
        "the published block budget left the arithmetic above"
    );
}

/// The oracle: encode a whole real block and confirm the codec agrees with the published budget.
///
/// This is the test that makes the budget independent of `capture`'s own arithmetic. It writes the
/// 254 full chunks and the 2-byte tail a real block splits into, closes the block the way the device
/// will, and sums the lengths the encoder actually returned.
///
/// It also computes the `AUDEND` checksum twice — once over the whole block as the host will, once
/// accumulated chunk by chunk as the device must, since a 32 KiB block lives in SDRAM and never
/// reaches the writer all at once. Those two agreeing is not a detail: if they ever disagreed, every
/// `AUDEND` from a real board would read as corrupt to a correct decoder.
#[test]
fn a_real_255_chunk_block_costs_what_the_module_claims() {
    let payload = [0xA5u8; capture::RING_BLOCK_BYTES];
    let chunks = capture::chunks_per_block() as u16;
    let mut wire_bytes = 0usize;
    let mut offset = 0usize;
    let mut chained_crc = CRC16_INITIAL;

    while offset < payload.len() {
        let end = (offset + CHUNK_RAW).min(payload.len());
        let chunk = &payload[offset..end];
        wire_bytes += audio_frame_len(0, chunks, (offset / CHUNK_RAW) as u16, chunk);
        chained_crc = crc16_ccitt_update(chained_crc, chunk);
        offset = end;
    }

    // Covering the block is a precondition, not a detail: a loop that stopped short would produce a
    // smaller total and could still land on the claimed figure by coincidence.
    assert_eq!(offset, capture::RING_BLOCK_BYTES);

    let crc = crc16_ccitt(&payload);
    assert_eq!(
        chained_crc, crc,
        "the chunk-wise CRC the device computes disagrees with the whole-block CRC the host checks"
    );
    wire_bytes += audend_frame_len(0, chunks, offset as u32, crc);

    assert_eq!(
        wire_bytes,
        capture::wire_bytes_per_block(),
        "the encoder put {} bytes on the wire for one block; capture claims {}",
        wire_bytes,
        capture::wire_bytes_per_block()
    );
}

/// Chaining agrees with the one-shot form at every split, including the ones an implementation
/// could plausibly get wrong: nothing, everything, and one byte either side of each boundary.
///
/// Run on a small buffer on purpose. The 32 KiB case above covers realistic geometry once; what
/// needs exhaustive coverage is the *arithmetic*, which does not grow with the message.
#[test]
fn chaining_the_crc_at_any_split_matches_one_shot() {
    let data = b"123456789";
    let whole = crc16_ccitt(data);
    assert_eq!(whole, 0x29b1, "and the check vector still holds");

    for split in 0..=data.len() {
        let (head, tail) = data.split_at(split);
        let chained = crc16_ccitt_update(crc16_ccitt_update(CRC16_INITIAL, head), tail);
        assert_eq!(chained, whole, "split at {split}");
    }

    // Splitting into single bytes is the degenerate shape a byte-at-a-time producer would use, and
    // it must survive too — 9 rounds of the same register.
    let drip = data
        .iter()
        .fold(CRC16_INITIAL, |acc, &b| crc16_ccitt_update(acc, &[b]));
    assert_eq!(drip, whole);

    // Zero-length pieces are no-ops, so a caller that loops over an empty chunk cannot corrupt a
    // running checksum.
    assert_eq!(crc16_ccitt_update(whole, b""), whole);
}

/// The useful fraction is 567 per 1000 at block level — and legitimately below the 568 per record.
#[test]
fn useful_fraction_is_567_per_mille_at_block_level() {
    let per_mille =
        (capture::RING_BLOCK_BYTES as u64 * 1_000 / capture::wire_bytes_per_block() as u64) as u32;
    assert_eq!(per_mille, 567);
    assert_eq!(
        capture::useful_fraction_per_mille(),
        per_mille,
        "the published block-level fraction left the arithmetic above"
    );

    // The per-record figure `dump` publishes and that `published_efficiency_matches_the_encoder`
    // pins. Different granularity, different denominator: a block additionally pays for its AUDEND
    // and its short final chunk. Pin both and keep them apart, so neither drifts into implying the
    // other.
    let per_record = (CHUNK_RAW as u64 * 1_000 / FULL_AUDIO_FRAME_LEN as u64) as u32;
    assert_eq!(per_record, 568, "dump's module doc says 0.568 per record");
    assert!(
        capture::useful_fraction_per_mille() < per_record,
        "a block-level fraction that reached the per-record figure would mean the framing stopped costing anything"
    );
}

/// The five-minute window TASK-019.03 asks for fits the ring, so ring-full stays a backstop.
#[test]
fn a_five_minute_capture_fits_the_ring() {
    assert_eq!(capture::CAPTURE_WINDOW_SECONDS, 300);
    assert_eq!(capture::expected_blocks(300), 879);
    assert!(
        capture::expected_blocks(capture::CAPTURE_WINDOW_SECONDS) < capture::RING_BLOCKS,
        "the default window no longer fits the ring"
    );

    // Rounding up is the whole point: 300 s of audio does not fit in 878 blocks, so an estimate that
    // reported 878 would understate the ring needed and could bless a run that overruns.
    assert_eq!(capture::expected_blocks(0), 0);
    assert_eq!(capture::expected_blocks(1), 3);

    // The same ceiling the duration helpers state, reached from the other side: 349 s still fits, one
    // second more does not.
    assert!(capture::expected_blocks(349) < capture::RING_BLOCKS);
    assert!(capture::expected_blocks(350) > capture::RING_BLOCKS);
}

// ---------------------------------------------------------------------------
// Wrap arithmetic
// ---------------------------------------------------------------------------

/// Block indices stay inside the ring under the mask the firmware will use.
#[test]
fn wrap_arithmetic_stays_inside_the_ring() {
    for index in [0usize, 1, 512, capture::RING_BLOCKS - 1] {
        let start = (index % capture::RING_BLOCKS) * capture::RING_BLOCK_BYTES;
        assert!(start + capture::RING_BLOCK_BYTES <= capture::RING_BYTES);
    }

    assert_eq!(1024 % capture::RING_BLOCKS, 0);
    assert_eq!(
        capture::RING_BLOCKS * capture::RING_BLOCK_BYTES,
        capture::RING_BYTES
    );
    // Powers of two are what let the firmware mask instead of dividing on the hot path.
    assert!(capture::RING_BLOCK_BYTES.is_power_of_two());
    assert!(capture::RING_BYTES.is_power_of_two());
}

// ---------------------------------------------------------------------------
// Block-state contract
// ---------------------------------------------------------------------------

/// Exactly the four hand-off edges are legal, out of all sixteen pairs.
#[test]
fn only_the_four_hand_off_edges_are_legal() {
    let mut legal = 0usize;
    for &from in &BlockState::ALL {
        for &to in &BlockState::ALL {
            let expected = LEGAL_EDGES.contains(&(from, to));
            assert_eq!(
                capture::transition_ok(from, to),
                expected,
                "transition {:?} -> {:?} disagrees with the hand-off table",
                from,
                to
            );
            if expected {
                legal += 1;
            }
        }
    }
    assert_eq!(legal, 4, "the state machine gained or lost an edge");
}

/// The machine is one cycle: no self-transition, and no producer or consumer shortcut.
#[test]
fn the_machine_is_one_cycle_with_no_shortcut_to_full() {
    for &state in &BlockState::ALL {
        assert!(
            !capture::transition_ok(state, state),
            "{:?} allowed a self-transition, which hides a lost update",
            state
        );
    }

    // Consequences of the two rules the edges enforce: the producer writes only a block it found
    // Free, the consumer reads only one it found Full.
    assert!(!capture::transition_ok(BlockState::Free, BlockState::Full));
    assert!(!capture::transition_ok(
        BlockState::Free,
        BlockState::Dumping
    ));
    assert!(!capture::transition_ok(
        BlockState::Full,
        BlockState::Filling
    ));
    assert!(!capture::transition_ok(
        BlockState::Dumping,
        BlockState::Filling
    ));

    // And every state has exactly one way out, so a stuck block has one cause to look for.
    for &from in &BlockState::ALL {
        let exits = BlockState::ALL
            .iter()
            .filter(|&&to| capture::transition_ok(from, to))
            .count();
        assert_eq!(exits, 1, "{:?} has {} successors, not one", from, exits);
    }
}

/// States round-trip through the stored byte, and a byte that names no state stays unrecognised.
#[test]
fn state_round_trips_through_u8_and_refuses_garbage() {
    let discriminants: [u8; 4] = [
        BlockState::Free.as_u8(),
        BlockState::Filling.as_u8(),
        BlockState::Full.as_u8(),
        BlockState::Dumping.as_u8(),
    ];
    assert_eq!(discriminants, [0, 1, 2, 3]);

    for &state in &BlockState::ALL {
        assert_eq!(BlockState::from_u8(state.as_u8()), Some(state));
    }

    // Never `unwrap_or(Free)`: reading an unrecognised status as free hands a block that may be
    // carrying audio, or mid-dump, to whoever comes next.
    assert_eq!(BlockState::from_u8(4), None);
    assert_eq!(BlockState::from_u8(u8::MAX), None);
}
