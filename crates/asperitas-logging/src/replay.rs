//! Replay staging core: an internal-RAM ping-pong between a QSPI refill task and the audio
//! callback, with the underrun policy and the DAC-path CRC that make one replay pass reportable.
//!
//! The rig plays an excerpt stored in QSPI flash (`crate::excerpt`). The audio callback has about
//! 590 us of its 666 us block budget to spare and must never wait on flash, so the samples reach it
//! through two halves of a staging buffer: a thread-mode task reads the next stretch of the
//! excerpt into whichever half is idle (`Flash::read_async` over MDMA, wired in TASK-038.04.03.02)
//! while the callback pops 32-sample blocks out of the other one. Everything about that hand-off
//! that can be wrong without a board - which half each side may touch, what happens when the
//! refill is late, how the final partial block is padded, what the reported CRC covers - lives
//! here, with no hardware types, so it is proven on the host. The firmware side only moves bytes.
//!
//! # Who calls what
//!
//! | side | context | calls |
//! |---|---|---|
//! | refill | one thread-mode task | [`ReplayCore::start`], [`ReplayCore::claim_fill`], [`FillClaim::buffer`], [`FillClaim::finish`], [`ReplayCore::finish`] |
//! | callback | the SAI1 audio interrupt | [`ReplayCore::take_block`] |
//!
//! Exactly one of each. The core is `Sync` so one `static` can be shared between the two, but its
//! correctness argument is single-producer single-consumer: each mutable field below is written by
//! one side only, and the half states are the only hand-off.
//!
//! # Underrun policy
//!
//! When the callback finds its next half not yet [`HalfState::Ready`] it outputs a block of
//! silence, counts one underrun, and does **not** advance the stream. The alternatives are worse:
//! replaying the previous half's stale samples is audible garbage that the CRC would then have to
//! either include (making it unreachable from the host side) or silently skip; advancing past the
//! missing samples makes the CRC unreachable and hides the glitch. Stalling keeps stream order, so
//! a pass with underruns still produces the source CRC once the data finally arrives, and the
//! report's claim is the pair "CRC equal *and* zero underruns". An underrun before the very first
//! fill is the same event; the firmware avoids it by letting both halves become ready before it
//! starts the callback consuming.
//!
//! # What the CRC says, and what it does not
//!
//! [`PassReport::crc16`] is CRC-16/CCITT-FALSE ([`crate::frame::crc16_ccitt`]) over exactly the
//! little-endian `i16` samples [`ReplayCore::take_block`] handed out from the source: inserted
//! silence and the zero padding after the last sample are excluded. After a pass it equals the slot
//! header's PCM CRC ([`crate::excerpt`]) and the host's CRC of the WAV data chunk. It proves the
//! bytes reached the DAC encoder intact and in order. It says nothing about what comes back through
//! the codec and the cable; that is TASK-035's measurement, not this one.
//!
//! # Staging depth
//!
//! The constants below are a justification table evaluated at compile time. A half must play for
//! at least [`MARGIN_FACTOR`] times the worst-case time to refill the other one, and the build
//! fails if it does not. Each input is labelled ASSUMED or MEASURED; today every refill input is
//! assumed, and TASK-038.09's bench reading is what replaces them. If a measured figure degrades
//! the margin, the assert is where [`HALF_BYTES`] has to change.
//!
//! Durations carry explicit widths with `u64` intermediates for the reason `crate::capture`'s
//! module docs record: const evaluation happens in the target's 32-bit `usize`.

use core::cell::UnsafeCell;
use core::ops::Range;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::capture;
use crate::frame::{crc16_ccitt_update, CRC16_INITIAL};

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Samples the callback pops per block: 32, `daisy_embassy::audio::BLOCK_LENGTH`.
///
/// Taken from [`capture::FRAMES_PER_CALLBACK`] rather than restated, so the capture ring and the
/// replay buffer cannot disagree about the callback they both serve.
pub const BLOCK_SAMPLES: usize = capture::FRAMES_PER_CALLBACK;

/// Bytes per replayed sample: mono `i16`, the excerpt format `crate::excerpt` stores.
pub const BYTES_PER_SAMPLE: usize = 2;

/// Source bytes one callback block consumes: 64.
pub const BLOCK_BYTES: usize = BLOCK_SAMPLES * BYTES_PER_SAMPLE;

/// Bytes in one staging half: 8 192, i.e. 128 blocks or 85.3 ms of audio.
///
/// Chosen as the smallest power of two that clears the margin assert below with the current
/// assumptions (4 096 bytes gives 42.7 ms against 4 x 14.1 ms = 56.4 ms and fails). Two halves cost
/// 16 KiB of AXI SRAM.
pub const HALF_BYTES: usize = 8_192;

/// Halves in the ping-pong: 2. One plays while the other refills.
pub const HALF_COUNT: usize = 2;

// ---------------------------------------------------------------------------
// Staging justification - every input labelled
// ---------------------------------------------------------------------------

/// Bytes the callback consumes per second: 96 000. MEASURED in the sense of fixed by
/// configuration: 48 kHz ([`capture::SAMPLE_RATE_HZ`]) x mono x 2 bytes.
pub const BYTES_PER_SECOND: u32 = capture::SAMPLE_RATE_HZ * BYTES_PER_SAMPLE as u32;

/// ASSUMED: worst-case delay between a half going [`HalfState::Empty`] and the refill task getting
/// to run, under console load on the thread-mode executor: 10 ms. No reading exists yet; the
/// executor also services the USB console, whose longest poll has not been timed.
pub const ASSUMED_SCHEDULING_LATENCY_MICROS: u32 = 10_000;

/// ASSUMED: lower bound on QSPI `read_async` throughput: 1 MB/s.
///
/// Deliberately pessimistic. The QUADSPI kernel clock is unset in the HAL config, so the real rate
/// is unknown; quad mode at 60 MHz would peak near 30 MB/s. 1 MB/s is a floor no working
/// configuration should be under.
pub const ASSUMED_QSPI_BYTES_PER_SECOND: u32 = 1_000_000;

/// ASSUMED: the margin a half's play time must exceed the refill time by: 4x. Covers
/// the unmeasured pieces above being off by a small integer factor without a glitch.
pub const MARGIN_FACTOR: u32 = 4;

/// How long one half plays: 85 333 us.
#[must_use]
pub const fn half_duration_micros() -> u32 {
    (HALF_BYTES as u64 * 1_000_000 / BYTES_PER_SECOND as u64) as u32
}

/// Time to read one half from QSPI at the assumed throughput: 8 192 us.
#[must_use]
pub const fn half_transfer_micros() -> u32 {
    (HALF_BYTES as u64 * 1_000_000).div_ceil(ASSUMED_QSPI_BYTES_PER_SECOND as u64) as u32
}

/// Worst-case refill latency for one half: scheduling plus transfer, 18 192 us.
#[must_use]
pub const fn worst_refill_micros() -> u32 {
    ASSUMED_SCHEDULING_LATENCY_MICROS + half_transfer_micros()
}

// The load-bearing gate: a half plays for at least MARGIN_FACTOR refills. 85 333 >= 72 768 today.
const _: () = assert!(
    half_duration_micros() as u64 >= MARGIN_FACTOR as u64 * worst_refill_micros() as u64,
    "a staging half plays for less than the required margin over the worst-case refill"
);

// A half is whole callback blocks, so only the very last block of a pass can be partial and a block
// never straddles two halves.
const _: () = assert!(
    HALF_BYTES.is_multiple_of(BLOCK_BYTES),
    "a staging half is not a whole number of callback blocks"
);

// Halves start on 32-byte boundaries (HalfBuf's alignment) and stay so, which is what lets the
// firmware invalidate exactly one half's cache lines after an MDMA write.
const _: () = assert!(
    HALF_BYTES.is_multiple_of(32),
    "a staging half is not a whole number of Cortex-M7 cache lines"
);

const _: () = assert!(
    HALF_COUNT == 2,
    "the ping-pong alternation assumes two halves"
);

const _: () = assert!(
    HALF_BYTES <= u32::MAX as usize,
    "half offsets are stored in u32"
);

// ---------------------------------------------------------------------------
// Half lifecycle
// ---------------------------------------------------------------------------

/// Lifecycle of one staging half, stored as a `u8` in the core's `AtomicU8` pair.
///
/// The same claim/publish discipline as [`capture::BlockState`]: the refill side writes only a half
/// it claimed from `Empty`, the callback reads only a half it adopted from `Ready`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HalfState {
    /// Drained by the callback; the refill side may claim it.
    Empty = 0,
    /// Claimed by the refill side and being written; the callback must not read it.
    Filling = 1,
    /// Written and published; the next half the callback will adopt.
    Ready = 2,
    /// Adopted by the callback and being played; the refill side may not touch it.
    Playing = 3,
}

impl HalfState {
    /// Every state, in discriminant order.
    pub const ALL: [HalfState; 4] = [
        HalfState::Empty,
        HalfState::Filling,
        HalfState::Ready,
        HalfState::Playing,
    ];

    /// The value stored in the state byte.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// The state a byte names, or `None`. No fallback to `Empty`: a corrupted byte read as `Empty`
    /// would hand a half under playback to the writer.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<HalfState> {
        match value {
            0 => Some(HalfState::Empty),
            1 => Some(HalfState::Filling),
            2 => Some(HalfState::Ready),
            3 => Some(HalfState::Playing),
            _ => None,
        }
    }
}

/// Whether a half may move from `from` to `to`.
///
/// Sixteen explicit arms and no wildcard, so a fifth state fails the build here. The four legal
/// edges, in order: refill claims, refill publishes, callback adopts, callback drains.
#[must_use]
pub const fn transition_ok(from: HalfState, to: HalfState) -> bool {
    match (from, to) {
        (HalfState::Empty, HalfState::Filling) => true,
        (HalfState::Filling, HalfState::Ready) => true,
        (HalfState::Ready, HalfState::Playing) => true,
        (HalfState::Playing, HalfState::Empty) => true,
        (HalfState::Empty, HalfState::Empty) => false,
        (HalfState::Empty, HalfState::Ready) => false,
        (HalfState::Empty, HalfState::Playing) => false,
        (HalfState::Filling, HalfState::Empty) => false,
        (HalfState::Filling, HalfState::Filling) => false,
        (HalfState::Filling, HalfState::Playing) => false,
        (HalfState::Ready, HalfState::Empty) => false,
        (HalfState::Ready, HalfState::Filling) => false,
        (HalfState::Ready, HalfState::Ready) => false,
        (HalfState::Playing, HalfState::Filling) => false,
        (HalfState::Playing, HalfState::Ready) => false,
        (HalfState::Playing, HalfState::Playing) => false,
    }
}

/// Move `cell` from `from` to `to` if it currently holds `from`. Every call site names a legal
/// edge; the debug assert catches a new one that does not.
fn advance(cell: &AtomicU8, from: HalfState, to: HalfState, success: Ordering) -> bool {
    debug_assert!(transition_ok(from, to));
    cell.compare_exchange(from.as_u8(), to.as_u8(), success, Ordering::Relaxed)
        .is_ok()
}

// ---------------------------------------------------------------------------
// Pass phase
// ---------------------------------------------------------------------------

const PHASE_IDLE: u8 = 0;
const PHASE_RUNNING: u8 = 1;
const PHASE_DONE: u8 = 2;

// ---------------------------------------------------------------------------
// The core
// ---------------------------------------------------------------------------

/// One staging half's storage, aligned to the Cortex-M7's 32-byte cache line so cache maintenance
/// after a DMA write touches this half and nothing else.
#[repr(C, align(32))]
struct HalfBuf([u8; HALF_BYTES]);

/// What one block of [`ReplayCore::take_block`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// Source samples were handed out; the final block of a pass may be zero-padded.
    Played,
    /// The next half was not ready: silence was handed out and the stream did not advance.
    Underrun,
    /// No pass is running (never started, or complete): silence, and no underrun counted.
    Idle,
}

/// Why [`ReplayCore::start`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// A pass is already running; let it complete first.
    Running,
    /// The length is not a whole number of 16-bit samples.
    OddLength {
        /// The refused length.
        bytes: u32,
    },
}

/// The outcome of one complete pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassReport {
    /// CRC-16/CCITT-FALSE over every source sample handed to the DAC encoder, in order.
    pub crc16: u16,
    /// Source samples handed out: the excerpt's length in samples once the pass is complete.
    pub samples: u32,
    /// Blocks of silence inserted because a half was late, saturating at `u32::MAX`.
    pub underruns: u32,
}

impl PassReport {
    /// Whether the pass played without a single underrun. A CRC match on an unclean pass proves
    /// the data arrived intact, not that it played without gaps.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.underruns == 0
    }
}

/// The ping-pong staging buffer and its accounting. See the module docs for the protocol.
///
/// 16 KiB plus a few words; meant to live in a `static` the firmware places in AXI SRAM.
pub struct ReplayCore {
    halves: [UnsafeCell<HalfBuf>; HALF_COUNT],
    states: [AtomicU8; HALF_COUNT],
    /// Bytes of source data in each half, written by the refill side before it publishes `Ready`.
    valid: [AtomicU32; HALF_COUNT],
    /// Excerpt length in bytes. Written only by [`ReplayCore::start`] while the callback is idle.
    pcm_bytes: AtomicU32,
    /// `PHASE_IDLE`, `PHASE_RUNNING` or `PHASE_DONE`: the start/finish hand-off.
    phase: AtomicU8,

    // Refill side only.
    /// Next source byte offset the refill side will claim.
    fill_offset: AtomicU32,
    /// Half the refill side claims next.
    fill_half: AtomicU8,

    // Callback side only.
    /// Half the callback plays next.
    play_half: AtomicU8,
    /// Byte offset inside the playing half.
    play_cursor: AtomicU32,
    /// Source bytes handed out so far this pass.
    consumed: AtomicU32,
    /// Running CRC, held in the low 16 bits.
    crc: AtomicU32,
    underruns: AtomicU32,
}

// SAFETY: the only non-Sync field is `halves`. Each half is written only through a `FillClaim`,
// which exists only while that half's state is `Filling` (taken by compare-exchange, so at most one
// claim per half), and read only by `take_block` while the state is `Playing`, which it reaches
// only from `Ready` after the claim's `finish` published with `Release`. The two sides therefore
// never touch the same half's bytes at the same time, and the Acquire/Release pairs on the state
// byte order the byte accesses around the hand-off.
unsafe impl Sync for ReplayCore {}

impl Default for ReplayCore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayCore {
    /// An idle core: [`take_block`](Self::take_block) hands out silence until
    /// [`start`](Self::start).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            halves: [
                UnsafeCell::new(HalfBuf([0; HALF_BYTES])),
                UnsafeCell::new(HalfBuf([0; HALF_BYTES])),
            ],
            states: [
                AtomicU8::new(HalfState::Empty as u8),
                AtomicU8::new(HalfState::Empty as u8),
            ],
            valid: [AtomicU32::new(0), AtomicU32::new(0)],
            pcm_bytes: AtomicU32::new(0),
            phase: AtomicU8::new(PHASE_IDLE),
            fill_offset: AtomicU32::new(0),
            fill_half: AtomicU8::new(0),
            play_half: AtomicU8::new(0),
            play_cursor: AtomicU32::new(0),
            consumed: AtomicU32::new(0),
            crc: AtomicU32::new(CRC16_INITIAL as u32),
            underruns: AtomicU32::new(0),
        }
    }

    /// Begin a pass over `pcm_bytes` bytes of source. Called by the refill side.
    ///
    /// Refused while a pass is running. Every half is `Empty` whenever no pass runs - a complete
    /// pass drains the halves it filled and claims nothing past the end - so resetting here touches
    /// no half the callback could be reading; the callback reads only the phase byte until the
    /// final `Release` store below publishes the reset. A zero-length pass completes immediately
    /// with the CRC of no bytes.
    pub fn start(&self, pcm_bytes: u32) -> Result<(), StartError> {
        if self.phase.load(Ordering::Acquire) == PHASE_RUNNING {
            return Err(StartError::Running);
        }
        if !pcm_bytes.is_multiple_of(BYTES_PER_SAMPLE as u32) {
            return Err(StartError::OddLength { bytes: pcm_bytes });
        }
        for (state, valid) in self.states.iter().zip(&self.valid) {
            state.store(HalfState::Empty.as_u8(), Ordering::Relaxed);
            valid.store(0, Ordering::Relaxed);
        }
        self.pcm_bytes.store(pcm_bytes, Ordering::Relaxed);
        self.fill_offset.store(0, Ordering::Relaxed);
        self.fill_half.store(0, Ordering::Relaxed);
        self.play_half.store(0, Ordering::Relaxed);
        self.play_cursor.store(0, Ordering::Relaxed);
        self.consumed.store(0, Ordering::Relaxed);
        self.crc.store(CRC16_INITIAL as u32, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
        let phase = if pcm_bytes == 0 {
            PHASE_DONE
        } else {
            PHASE_RUNNING
        };
        self.phase.store(phase, Ordering::Release);
        Ok(())
    }

    /// Claim the next half to refill, or `None` when there is nothing to do: no pass running, the
    /// whole source already claimed, a claim already outstanding, or the next half still queued or
    /// playing. Called by the refill side, which retries after the callback drains a half.
    ///
    /// Halves are claimed strictly alternately, so the callback, which also alternates, plays
    /// the source in order.
    pub fn claim_fill(&self) -> Option<FillClaim<'_>> {
        if self.phase.load(Ordering::Acquire) != PHASE_RUNNING {
            return None;
        }
        let pcm_bytes = self.pcm_bytes.load(Ordering::Relaxed);
        let offset = self.fill_offset.load(Ordering::Relaxed);
        if offset >= pcm_bytes {
            return None;
        }
        let half = usize::from(self.fill_half.load(Ordering::Relaxed));
        // Acquire pairs with the callback's Release when it drained this half, so its last reads of
        // the old bytes happen before the claim's writes.
        if !advance(
            &self.states[half],
            HalfState::Empty,
            HalfState::Filling,
            Ordering::Acquire,
        ) {
            return None;
        }
        let len = (pcm_bytes - offset).min(HALF_BYTES as u32);
        Some(FillClaim {
            core: self,
            half,
            source: offset..offset + len,
        })
    }

    /// Hand the DAC encoder the next 32 samples. Called by the audio callback.
    ///
    /// A few loads, one CRC update over 64 bytes, and 32 stores: no flash access, no locks, no
    /// loops on the other side's progress.
    pub fn take_block(&self, out: &mut [i16; BLOCK_SAMPLES]) -> Block {
        if self.phase.load(Ordering::Acquire) != PHASE_RUNNING {
            out.fill(0);
            return Block::Idle;
        }
        let half = usize::from(self.play_half.load(Ordering::Relaxed));
        let cursor = self.play_cursor.load(Ordering::Relaxed);
        if cursor == 0
            && !advance(
                &self.states[half],
                HalfState::Ready,
                HalfState::Playing,
                Ordering::Acquire,
            )
        {
            out.fill(0);
            let underruns = self.underruns.load(Ordering::Relaxed);
            self.underruns
                .store(underruns.saturating_add(1), Ordering::Relaxed);
            return Block::Underrun;
        }
        let valid = self.valid[half].load(Ordering::Relaxed);
        let n = (valid - cursor).min(BLOCK_BYTES as u32);
        let start = cursor as usize;
        // SAFETY: the half is `Playing`, adopted above or on an earlier block, so no `FillClaim`
        // for it exists and nothing writes its bytes until this side stores `Empty` below.
        let bytes = unsafe { &(&(*self.halves[half].get()).0)[start..start + n as usize] };
        for (sample, pair) in out.iter_mut().zip(bytes.chunks_exact(BYTES_PER_SAMPLE)) {
            *sample = i16::from_le_bytes([pair[0], pair[1]]);
        }
        out[n as usize / BYTES_PER_SAMPLE..].fill(0);
        let crc = crc16_ccitt_update(self.crc.load(Ordering::Relaxed) as u16, bytes);
        self.crc.store(u32::from(crc), Ordering::Relaxed);

        let consumed = self.consumed.load(Ordering::Relaxed) + n;
        self.consumed.store(consumed, Ordering::Relaxed);
        let cursor = cursor + n;
        if cursor == valid {
            self.play_cursor.store(0, Ordering::Relaxed);
            self.play_half
                .store(((half + 1) % HALF_COUNT) as u8, Ordering::Relaxed);
            // Release: this block's reads of the half happen before the refill side's next writes.
            let drained = advance(
                &self.states[half],
                HalfState::Playing,
                HalfState::Empty,
                Ordering::Release,
            );
            debug_assert!(drained, "a playing half changed state under the callback");
        } else {
            self.play_cursor.store(cursor, Ordering::Relaxed);
        }
        if consumed >= self.pcm_bytes.load(Ordering::Relaxed) {
            // Release: the CRC, sample and underrun stores above are visible to `finish`.
            self.phase.store(PHASE_DONE, Ordering::Release);
        }
        Block::Played
    }

    /// Underruns so far this pass, readable at any time.
    #[must_use]
    pub fn underruns(&self) -> u32 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// The pass's report once every source sample has been handed out, otherwise `None`.
    ///
    /// Stays readable until the next [`start`](Self::start), so a reporter that missed the moment
    /// of completion loses nothing.
    #[must_use]
    pub fn finish(&self) -> Option<PassReport> {
        if self.phase.load(Ordering::Acquire) != PHASE_DONE {
            return None;
        }
        Some(PassReport {
            crc16: self.crc.load(Ordering::Relaxed) as u16,
            samples: self.consumed.load(Ordering::Relaxed) / BYTES_PER_SAMPLE as u32,
            underruns: self.underruns.load(Ordering::Relaxed),
        })
    }
}

/// The refill side's exclusive hold on one `Filling` half.
///
/// Write the [`source`](Self::source) range of the excerpt into [`buffer`](Self::buffer), then
/// [`finish`](Self::finish). Dropping a claim unfinished leaves the half `Filling` and the
/// callback underrunning on it from then on, audibly and counted; there is no legal edge back
/// to `Empty`, because a read that failed halfway has no defined contents to retract.
#[must_use = "a claimed half plays only once finished"]
pub struct FillClaim<'a> {
    core: &'a ReplayCore,
    half: usize,
    source: Range<u32>,
}

impl FillClaim<'_> {
    /// Which half (0 or 1) this claim writes.
    #[must_use]
    pub fn half(&self) -> usize {
        self.half
    }

    /// The excerpt byte range to read into [`buffer`](Self::buffer), relative to the slot's PCM
    /// start. Its length is [`HALF_BYTES`] except for the pass's final half.
    #[must_use]
    pub fn source(&self) -> Range<u32> {
        self.source.clone()
    }

    /// The half's storage, exactly [`source`](Self::source)'s length long, 32-byte aligned.
    pub fn buffer(&mut self) -> &mut [u8] {
        let len = (self.source.end - self.source.start) as usize;
        // SAFETY: this claim exists only while the half is `Filling`, which `claim_fill` took by
        // compare-exchange, so it is the sole accessor of these bytes; `&mut self` keeps the
        // returned slice from outliving it or aliasing a second call.
        unsafe { &mut (&mut (*self.core.halves[self.half].get()).0)[..len] }
    }

    /// Publish the half as `Ready` with every byte of [`source`](Self::source) written.
    ///
    /// Takes no length: the claim already fixed it, and a second source of truth for how many
    /// bytes landed could only disagree with the first.
    pub fn finish(self) {
        let core = self.core;
        let len = self.source.end - self.source.start;
        core.valid[self.half].store(len, Ordering::Relaxed);
        core.fill_offset.store(self.source.end, Ordering::Relaxed);
        core.fill_half
            .store(((self.half + 1) % HALF_COUNT) as u8, Ordering::Relaxed);
        // Release: the buffer writes and the length above happen before the callback's adoption.
        let published = advance(
            &core.states[self.half],
            HalfState::Filling,
            HalfState::Ready,
            Ordering::Release,
        );
        debug_assert!(published, "a filling half changed state under its claim");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn justification_table_evaluates_to_the_documented_figures() {
        assert_eq!(BYTES_PER_SECOND, 96_000);
        assert_eq!(BLOCK_BYTES, 64);
        assert_eq!(half_duration_micros(), 85_333);
        assert_eq!(half_transfer_micros(), 8_192);
        assert_eq!(worst_refill_micros(), 18_192);
        assert_eq!(MARGIN_FACTOR * worst_refill_micros(), 72_768);
    }

    #[test]
    fn half_buffers_are_cache_line_aligned() {
        let core = ReplayCore::new();
        for half in &core.halves {
            assert_eq!(half.get() as usize % 32, 0);
        }
    }
}
