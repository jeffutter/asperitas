//! Capture-ring geometry, derived from the console dump grammar.
//!
//! One SDRAM ring and one console stream describe the same capture, so they must agree on the
//! numbers that size them. Before this module those numbers lived in two places that could not see
//! each other: [`crate::dump`] owned the wire grammar, and the ring's shape existed only as prose in
//! TASK-038.03's notes. Nothing checked that a 32 KiB ring block is *encodable* — and it is, by
//! exactly zero margin: [`RING_BLOCK_BYTES`] is 254 full chunks plus a 2-byte tail, i.e. 255 chunks,
//! which *is* [`crate::dump::MAX_CHUNKS_PER_BLOCK`]. Exceed that and [`crate::dump::audio_body`]
//! refuses at runtime mid-dump, discovered at the bench with ears on the line.
//!
//! Every figure here is therefore computed from `dump`'s own constants and gated against them at
//! compile time. An off-by-one fails `cargo build` on thumbv7em and `cargo test` on the host rather
//! than a capture, and widening a header moves the ring numbers instead of silently desynchronising
//! them. This module reads `dump`'s and [`crate::frame`]'s public constants and redefines none of
//! them.
//!
//! # Why the numbers live here
//!
//! The module's entire reason to exist is its coupling to the wire grammar, so putting it anywhere
//! else replaces that dependency with a restated constant. The `firmware/` package is a separate
//! workspace with no host-test target, so a literal there is checked only by a cross build with
//! nothing to compare against. Here the same consts are evaluated on both targets and
//! `tests/capture_geometry.rs` asserts the result against an oracle that encodes a real block
//! through [`crate::dump::audio_record`] and [`crate::dump::audend_record`].
//!
//! It is therefore an *ungated* module like `dump`: no `log-usb`, no `critical_section`, no
//! `embassy-time` types, so it compiles and gets tested under default features on the host and on
//! thumbv7em alike. Those types fail at link time off-target, which is why `LOG_PIPE_SIZE` is an
//! ungated bare `usize` while the pipe itself stays gated.
//!
//! # Integer widths follow the target, not the host
//!
//! Const evaluation happens in the *target's* environment, where `usize` is 32 bits on
//! thumbv7em-none-eabihf. Byte offsets and counts stay `usize` — the ring is 33.5 MB, which fits
//! `u32`, so nothing becomes unaddressable — while every duration carries an explicit width and
//! takes its intermediate products in `u64`. Written the other way round, `RING_BYTES * 1_000_000 /
//! BYTES_PER_SECOND` in `usize` builds clean on the host and fails the cross build with
//! `error[E0080]: attempt to compute 33554432_usize * 1000000_usize, which would overflow`: host
//! tests green, firmware red. That measured failure, not taste, is what the rule below exists for.
//!
//! # Three useful fractions, three granularities
//!
//! | figure | granularity | source |
//! |---|---|---|
//! | 0.568 | per record: [`crate::dump::CHUNK_RAW`] inside one full frame | `dump`'s module doc, pinned by `published_efficiency_matches_the_encoder` |
//! | 0.487 | per record, under the rejected descriptive-key header | `dump`'s module doc |
//! | [`useful_fraction_per_mille`] = 567 ‰ | per **block**: whole ring payload over every byte that block puts on the wire, AUDEND and tail chunk included | [`useful_fraction_per_mille`] |
//!
//! The block-level figure is legitimately *below* the per-record one because it additionally pays
//! for the closing `AUDEND` and the short final chunk. They are not equal and never will be;
//! `tests/capture_geometry.rs` pins both and asserts the strict inequality so neither can drift into
//! false agreement.

use crate::dump;
use crate::frame::{PREFIX_LEN, TRAILER_LEN};

// ---------------------------------------------------------------------------
// Hardware inputs
// ---------------------------------------------------------------------------

/// Sample rate in Hz: 48 000.
///
/// A deliberate copy of the rate the firmware configures — `daisy_embassy`'s `AudioConfig::default`
/// — because this module cannot name that type without dragging the HAL into a crate whose tests run
/// on the host. TASK-038.03.02 restores the link with a const assert in `rig.rs`, where both sides
/// are visible.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Bytes per captured sample: mono `i16`.
///
/// The rig stores the loop channel only (TASK-038.03.02), so the ring's footprint is half what a
/// stereo capture would cost; a second channel doubles [`BYTES_PER_SECOND`] and halves the ring's
/// seconds, which is why the figure is named rather than inlined.
pub const CAPTURE_BYTES_PER_SAMPLE: usize = 2;

/// Samples per audio callback per channel: 32.
///
/// A deliberate copy of `daisy_embassy::audio::BLOCK_LENGTH`, with the same reasoning as
/// [`SAMPLE_RATE_HZ`]. TASK-038.03.02 asserts equality against the real constant **in samples**.
/// Comparing [`CALLBACK_BYTES`] against `HALF_DMA_BUFFER_LENGTH` instead is a category error — that
/// constant counts 64 `u32` *words* per callback (32 stereo frames) while [`CALLBACK_BYTES`] counts
/// 64 *bytes* of one mono channel — and their numeric coincidence (64 == 64) is exactly why the
/// comparison must be made in samples rather than bytes.
pub const FRAMES_PER_CALLBACK: usize = 32;

/// Bytes one callback appends to the ring: 64.
pub const CALLBACK_BYTES: usize = FRAMES_PER_CALLBACK * CAPTURE_BYTES_PER_SAMPLE;

// ---------------------------------------------------------------------------
// Ring geometry
// ---------------------------------------------------------------------------

/// Raw bytes per ring block: 32 768 (32 KiB).
///
/// Provably optimal rather than chosen by taste. [`crate::dump::MAX_CHUNKS_PER_BLOCK`] makes
/// [`crate::dump::MAX_BLOCK_BYTES`] = 32 895 the widest payload the grammar can describe at all, the
/// next power of two (64 KiB) needs 509 chunks and is refused twice over — by the chunk count and by
/// the byte ceiling — and powers of two are forced by the wrap-mask gate below. That leaves 32 KiB
/// as the only candidate. Growing further would buy little regardless: the useful fraction rises
/// slowly with block size (8 KiB → 564, 16 KiB → 565, 32 KiB → 567 per 1000, all three computed from
/// the encoder) while the per-block state array doubles.
///
/// The slack against the grammar is 127 bytes ([`crate::dump::MAX_BLOCK_BYTES`] − this); the
/// *chunk* margin is zero, which is why [`chunks_per_block`]'s gate is the load-bearing one.
pub const RING_BLOCK_BYTES: usize = 32_768;

/// Blocks in the ring: 1 024.
///
/// Sized so a five-minute capture ([`CAPTURE_WINDOW_SECONDS`]) fits with room to spare, and small
/// enough that the per-block state array — one byte per block, held in internal RAM where roughly
/// 69 KB is free — stays comfortably inside [`u16::MAX`].
pub const RING_BLOCKS: usize = 1_024;

/// Total ring capacity in raw bytes: 33 554 432 (32 MiB).
///
/// Half the Seed3's 64 MB of SDRAM (`docs/reference/daisy-seed3.md`); the figure TASK-038.04's QSPI
/// staging buffer and TASK-038.06's budget documentation have to quote alongside this module.
pub const RING_BYTES: usize = RING_BLOCK_BYTES * RING_BLOCKS;

/// Ring footprint per second of capture in bytes: 96 000 for mono 16-bit at 48 kHz.
pub const BYTES_PER_SECOND: usize = SAMPLE_RATE_HZ as usize * CAPTURE_BYTES_PER_SAMPLE;

/// Default capture window in seconds: 300, the five minutes TASK-019.03 asks to be verified
/// objectively.
///
/// Gate 6 below fails the build if the ring ever stops fitting this window. A rig that raises the
/// window by `option_env!` override guards its own choice where that override is visible, not here.
pub const CAPTURE_WINDOW_SECONDS: usize = 300;

// ---------------------------------------------------------------------------
// Derived geometry
// ---------------------------------------------------------------------------

/// Audio callbacks written into one block: 512.
#[must_use]
pub const fn callbacks_per_block() -> usize {
    RING_BLOCK_BYTES / CALLBACK_BYTES
}

/// Captured samples per channel in one block: 16 384.
#[must_use]
pub const fn samples_per_block() -> usize {
    RING_BLOCK_BYTES / CAPTURE_BYTES_PER_SAMPLE
}

/// Whole [`crate::dump::CHUNK_RAW`]-sized chunks one block splits into: 254.
#[must_use]
pub const fn full_chunks_per_block() -> usize {
    RING_BLOCK_BYTES / dump::CHUNK_RAW
}

/// Raw bytes in the short final chunk: 2. Zero when a block divides evenly, in which case
/// [`tail_frame_bytes`] emits no final `AUDIO` record at all.
#[must_use]
pub const fn tail_chunk_bytes() -> usize {
    RING_BLOCK_BYTES % dump::CHUNK_RAW
}

/// `AUDIO` chunks one block needs: 255 — today exactly [`crate::dump::MAX_CHUNKS_PER_BLOCK`].
#[must_use]
pub const fn chunks_per_block() -> usize {
    RING_BLOCK_BYTES.div_ceil(dump::CHUNK_RAW)
}

/// Records one block puts on the wire: 256 — the chunks plus the closing `AUDEND`.
#[must_use]
pub const fn records_per_block() -> usize {
    chunks_per_block() + 1
}

/// Wire bytes for the final, short `AUDIO` record: 59. Zero when [`tail_chunk_bytes`] is zero, so
/// the formula survives a block size that divides evenly instead of emitting a phantom empty chunk.
///
/// Every numeric field renders fixed-width and zero-padded, so this is exact for any tail length
/// rather than a worst case: the header is [`crate::dump::AUDIO_HEADER_LEN`] whatever the chunk
/// carries, and [`crate::dump::encoded_len`] gives the padded base64 width of the payload.
#[must_use]
pub const fn tail_frame_bytes() -> usize {
    let tail = tail_chunk_bytes();
    if tail == 0 {
        0
    } else {
        PREFIX_LEN + dump::AUDIO_HEADER_LEN + dump::encoded_len(tail) + TRAILER_LEN
    }
}

/// Wire bytes for the closing `AUDEND` record: 71. Exact, for the same fixed-width reason as
/// [`tail_frame_bytes`]: `bytes` ships at full [`crate::dump::MAX_AUDEND_BODY_LEN`] whether the
/// block carried 2 bytes or 32 768.
#[must_use]
pub const fn audend_frame_bytes() -> usize {
    PREFIX_LEN + dump::MAX_AUDEND_BODY_LEN + TRAILER_LEN
}

/// Wire bytes one full ring block costs: 57 788 — always, not on average.
///
/// Full chunks at [`crate::dump::FULL_AUDIO_FRAME_LEN`], plus the short final chunk, plus the
/// `AUDEND`. Fixed-width fields make the sum exact; `tests/capture_geometry.rs` confirms it against
/// a real 255-chunk block encoded through [`crate::dump::audio_record`] and
/// [`crate::dump::audend_record`] rather than against this arithmetic.
#[must_use]
pub const fn wire_bytes_per_block() -> usize {
    full_chunks_per_block() * dump::FULL_AUDIO_FRAME_LEN + tail_frame_bytes() + audend_frame_bytes()
}

/// Payload bytes per 1000 wire bytes, measured **per block**: 567.
///
/// Lower than the 568 per-record figure `dump` publishes, because a block also pays for its
/// [`tail_frame_bytes`] and [`audend_frame_bytes`] — see the table in the module docs. Taken in
/// `u64`: the numerator is [`RING_BLOCK_BYTES`] × 1000, which is 32.7 million and so survives
/// 32-bit `usize` today but would not survive a wider block.
#[must_use]
pub const fn useful_fraction_per_mille() -> u32 {
    let scaled = RING_BLOCK_BYTES as u64 * 1_000;
    (scaled / wire_bytes_per_block() as u64) as u32
}

// ---------------------------------------------------------------------------
// Duration helpers — explicit widths, see the module docs
// ---------------------------------------------------------------------------

/// Whole seconds the ring can hold before it is full: 349.
///
/// Floored: the 349th second completes, the 350th does not.
#[must_use]
pub const fn ring_seconds_floor() -> u32 {
    let micros = ring_duration_micros();
    (micros / 1_000_000) as u32
}

/// Exact ring capacity in microseconds: 349 525 333 (349.5 s of mono 16-bit at 48 kHz).
///
/// The product is taken in `u64` on purpose — see the module docs on 32-bit const eval.
#[must_use]
pub const fn ring_duration_micros() -> u64 {
    let bytes = RING_BYTES as u64;
    bytes * 1_000_000 / BYTES_PER_SECOND as u64
}

/// Ring blocks a capture of `seconds` will consume: 879 for the default window.
///
/// Rounded **up**, because a run that spills into a block consumes that block: 300 s × 96 000 B/s
/// needs 879 blocks, and a helper that reported 878 would understate what the ring has to hold,
/// which is the one thing a capacity estimate may not do. The argument is `usize` to match the
/// index arithmetic at the call site (TASK-038.03.02's `rig.rs`); the product is still taken in
/// `u64`, since `seconds × BYTES_PER_SECOND` passes `u32::MAX` near 44 700 s and the const evaluator
/// rejects the 32-bit form outright rather than leaving it behind a `debug_assert`.
#[must_use]
pub const fn expected_blocks(seconds: usize) -> usize {
    let bytes = seconds as u64 * BYTES_PER_SECOND as u64;
    bytes.div_ceil(RING_BLOCK_BYTES as u64) as usize
}

// ---------------------------------------------------------------------------
// Compile-time gates — the reason this module exists
// ---------------------------------------------------------------------------

// 1. A ring block must be describable by the grammar at all. 127 bytes of slack today.
const _: () = assert!(
    RING_BLOCK_BYTES <= dump::MAX_BLOCK_BYTES,
    "a ring block carries more raw bytes than the dump grammar can describe"
);

// 2. The load-bearing gate: the chunk count is at the ceiling exactly (255 == 255), so this is the
//    assertion that breaks first when either side moves. Gate 1 has slack; this one has none.
const _: () = assert!(
    chunks_per_block() <= dump::MAX_CHUNKS_PER_BLOCK,
    "a ring block needs more AUDIO chunks than the 2-digit n field can name"
);

// 3. Blocks must align to whole callbacks, so a block boundary is never mid-callback and the
//    producer never splits a DMA half across two blocks.
const _: () = assert!(
    RING_BLOCK_BYTES.is_multiple_of(CALLBACK_BYTES),
    "a ring block is not a whole number of audio callbacks"
);

// 4. Powers of two keep wrap arithmetic a mask. kfifo, ringbuf and rtrb all make this a documented
//    requirement, so it is correctness rather than style: a non-power-of-two ring needs a modulo on
//    the hot path or a head/tail pair that can disagree.
const _: () = assert!(
    RING_BLOCK_BYTES.is_power_of_two() && RING_BYTES.is_power_of_two(),
    "ring geometry must stay powers of two for wrap arithmetic to stay a mask"
);

// 5. The per-block state array lives in internal RAM (~69 KB free), so block indices must fit a
//    u16 the firmware can store atomically.
const _: () = assert!(
    RING_BLOCKS <= u16::MAX as usize,
    "ring block indices no longer fit the u16 the state array stores"
);

// 6. The default capture window must fit the ring, so ring-full stays a backstop rather than the
//    normal end of a run. Shrinking the ring, or growing the window past what it holds, costs a
//    build rather than a bench run.
const _: () = assert!(
    expected_blocks(CAPTURE_WINDOW_SECONDS) < RING_BLOCKS,
    "the default capture window no longer fits the ring"
);

// Geometry the published figures depend on. These are not extra policy: they are the assumptions
// the derivation above makes about how a block splits, and each one failing would mean
// wire_bytes_per_block describes a block nobody can write.
const _: () = assert!(
    tail_chunk_bytes() < dump::CHUNK_RAW,
    "a block left a tail as large as a full chunk"
);
const _: () = assert!(
    full_chunks_per_block() * dump::CHUNK_RAW + tail_chunk_bytes() == RING_BLOCK_BYTES,
    "full chunks plus the tail do not cover a ring block"
);

// ---------------------------------------------------------------------------
// Block-state contract
// ---------------------------------------------------------------------------
//
// The producer/consumer hand-off for one ring block, published as data rather than as a comment, so
// the firmware's AtomicU8 array, its tests, and whoever debugs a stuck capture all read the same
// machine. The two rules its edges enforce: the producer writes only a block found Free, the
// consumer reads only one found Full. That is the same claim/publish discipline a Disruptor sequence
// documents, and for the same reason — the failure it prevents is handing a block carrying audio to
// a writer.

/// Lifecycle of one ring block.
///
/// Stored as a `u8` in the firmware's `AtomicU8` array — natively lock-free on ARMv7-M via
/// `LDREXB`/`STREXB`, so no `portable-atomic` is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlockState {
    /// Written by the consumer, ready for the producer to claim.
    Free = 0,
    /// Claimed by the producer and being filled; invisible to the consumer.
    Filling = 1,
    /// Fully written and checksummed: publishable, and the only state the consumer may read.
    Full = 2,
    /// Claimed by the consumer and on the wire; the producer may not touch it.
    Dumping = 3,
}

impl BlockState {
    /// Every state, in discriminant order — what the exhaustive host tests iterate.
    pub const ALL: [BlockState; 4] = [
        BlockState::Free,
        BlockState::Filling,
        BlockState::Full,
        BlockState::Dumping,
    ];

    /// The value stored in the firmware's status byte.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            BlockState::Free => 0,
            BlockState::Filling => 1,
            BlockState::Full => 2,
            BlockState::Dumping => 3,
        }
    }

    /// The state a status byte names, or `None` if it names none of them.
    ///
    /// Deliberately not `unwrap_or(Free)`: a corrupted byte that silently reads as `Free` hands a
    /// block carrying audio to a writer, which is the one failure this machine exists to prevent. A
    /// caller with a `None` in hand has a real overrun-or-corruption event to report, not a block to
    /// reuse. Hand-written rather than `num_enum`/`strum` because this crate keeps its dependency
    /// surface at `log` plus `cortex-m`.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<BlockState> {
        match value {
            0 => Some(BlockState::Free),
            1 => Some(BlockState::Filling),
            2 => Some(BlockState::Full),
            3 => Some(BlockState::Dumping),
            _ => None,
        }
    }
}

/// Whether a block may move from `from` to `to`.
///
/// Total over the 4×4 product, written as sixteen explicit arms with no wildcard: adding a fifth
/// state then fails the build at every arm instead of quietly falling through to `false`.
///
/// The four legal edges are the claim/publish hand-off in order — producer claims, producer
/// publishes, consumer claims, consumer releases. Everything else is refused, including every
/// self-transition: a writer that re-stamps a block it already owns is describing a lost update, not
/// a no-op.
#[must_use]
pub const fn transition_ok(from: BlockState, to: BlockState) -> bool {
    match (from, to) {
        // Producer claims a block it found free.
        (BlockState::Free, BlockState::Filling) => true,
        // Producer publishes a block whose last byte and checksum have landed.
        (BlockState::Filling, BlockState::Full) => true,
        // Consumer claims a block it found full.
        (BlockState::Full, BlockState::Dumping) => true,
        // Consumer releases a block once its AUDEND has been accepted.
        (BlockState::Dumping, BlockState::Free) => true,
        // Producer and consumer never skip a stage, never take a shortcut past Full, and never
        // re-enter a state they are already in.
        (BlockState::Free, BlockState::Free) => false,
        (BlockState::Free, BlockState::Full) => false,
        (BlockState::Free, BlockState::Dumping) => false,
        (BlockState::Filling, BlockState::Free) => false,
        (BlockState::Filling, BlockState::Filling) => false,
        (BlockState::Filling, BlockState::Dumping) => false,
        (BlockState::Full, BlockState::Free) => false,
        (BlockState::Full, BlockState::Filling) => false,
        (BlockState::Full, BlockState::Full) => false,
        (BlockState::Dumping, BlockState::Filling) => false,
        (BlockState::Dumping, BlockState::Full) => false,
        (BlockState::Dumping, BlockState::Dumping) => false,
    }
}
