//! The capture ring in SDRAM and the state shared between the audio callback and the dump writer.
//!
//! One producer (the audio callback at P6) and one consumer (the dump writer in thread mode) share
//! the ring through a byte of state per block and nothing else. The producer claims a `Free` block,
//! fills it, and publishes it `Full` with release ordering; the writer claims a `Full` block with
//! acquire ordering, ships it, and hands it back `Free` with release ordering. Payload is always
//! written before the state byte that publishes it, so whoever observes the new state also observes
//! the bytes. That is the classic SPSC discipline, and it is why this module does not reuse
//! `src/bin/main.rs`'s `UnsafeCell` + `interrupt::free` idiom: a critical section is right for one slow knob
//! writer and wrong for handing buffer ownership across an interrupt boundary 1 500 times a second.
//!
//! The ring is uncached by memory type, not by luck: no MPU region covers 0xC000_0000 (daisy-embassy
//! puts its cacheable region at 0xD000_0000, an unconfigured FMC bank), so the default map makes the
//! window Device memory, and enabling the D-cache does not touch it. That was measured off the
//! running board (docs/reference/daisy-seed3.md, "External SDRAM"). What would change
//! this hand-off is an MPU region over 0xC000_0000: whoever adds one owns the coherence argument for
//! the FMC window and revisits these orderings in the same change. Device memory also takes no
//! unaligned accesses, so the ring is only ever touched through aligned copies.

use crate::geometry::{MONO_WORD, RING_FILL};
use crate::report::now_ms;
use asperitas_wire::capture::{self, BlockState};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use cortex_m::peripheral::DWT;

/// Base address of the ring in SDRAM, as handed back by `Sdram::init`.
///
/// Carried as a raw pointer rather than a slice on purpose: producer and writer hold different
/// blocks of the same 32 MiB at the same time, and two `&mut [u8]` over one allocation would be
/// aliasing whatever the block states say. Each side builds a slice over exactly one block, only
/// after the state byte has made that block its own.
#[derive(Clone, Copy)]
pub(crate) struct Ring {
    base: *mut u8,
}

// Safety: the pointer names memory-mapped SDRAM that lives for the program's lifetime, and every
// access through it is gated by `BLOCK_STATE`, which hands each block to exactly one side.
unsafe impl Send for Ring {}

impl Ring {
    pub(crate) fn new(base: *mut u8) -> Self {
        Self { base }
    }

    /// The bytes of block `index`.
    ///
    /// # Safety
    /// The caller must own the block through [`BLOCK_STATE`] - `Filling` for the producer,
    /// `Dumping` for the writer - for as long as the slice lives.
    #[allow(clippy::mut_from_ref)] // Ownership comes from the state byte, not from `&self`.
    pub(crate) unsafe fn block(&self, index: usize) -> &mut [u8] {
        debug_assert!(index < capture::RING_BLOCKS);
        unsafe {
            core::slice::from_raw_parts_mut(
                self.base.add(index * capture::RING_BLOCK_BYTES),
                capture::RING_BLOCK_BYTES,
            )
        }
    }
}

/// One state byte per ring block, holding a `capture::BlockState`. 1 KiB of internal RAM.
pub(crate) static BLOCK_STATE: [AtomicU8; capture::RING_BLOCKS] =
    [const { AtomicU8::new(BlockState::Free.as_u8()) }; capture::RING_BLOCKS];

/// Move block `index` from `from` to `to`, if it is in `from`. Returns whether it was.
///
/// The only way any block state changes, so the edge check against `capture::transition_ok` sits in
/// one place and covers producer and writer alike. Acquire on success makes the previous owner's
/// writes visible before this side touches the block; release makes this side's writes visible to
/// the next owner.
pub(crate) fn advance(index: usize, from: BlockState, to: BlockState) -> bool {
    debug_assert!(
        capture::transition_ok(from, to),
        "block state edge not in capture::transition_ok"
    );
    BLOCK_STATE[index]
        .compare_exchange(
            from.as_u8(),
            to.as_u8(),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

/// Blocks published `Full` since boot. Never reset: the ring index of block `n` is
/// `n % RING_BLOCKS`, and the dump labels each block with `n`.
pub(crate) static PRODUCED: AtomicU32 = AtomicU32::new(0);
/// Blocks the producer refused to claim because the next one in ring order was not `Free`. A
/// count of refusals, not of bytes: the first one stops the capture, so it reads 0 or 1 per run.
pub(crate) static OVERRUN: AtomicU32 = AtomicU32::new(0);
/// Blocks whose `AUDEND` has been committed to the console.
pub(crate) static DUMPED: AtomicU32 = AtomicU32::new(0);
/// Set by the capture timeline to start a capture, cleared by it at the deadline and by the
/// producer on overrun. The producer reads it only at a block boundary.
pub(crate) static ARMED: AtomicBool = AtomicBool::new(false);
/// Whether the producer currently owns a `Filling` block.
pub(crate) static FILLING: AtomicBool = AtomicBool::new(false);
/// Set by the capture timeline immediately before it arms and cleared only after its dump has
/// ended, so it covers armed, filling, the drain of the last block, judging and dumping as one span
/// with no gap between them. The excerpt installer refuses to touch flash while it is set. `ARMED`
/// alone would not do: the producer clears it on overrun, minutes before the dump that follows.
pub(crate) static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Set by the first audio callback.
pub(crate) static AUDIO_STARTED: AtomicBool = AtomicBool::new(false);
/// Longest callback and longest gap between callback starts, in raw DWT cycles. Converted to
/// microseconds only when reported, with the time base boot settled on.
pub(crate) static MAX_BLOCK_CYCLES: AtomicU32 = AtomicU32::new(0);
pub(crate) static WORST_GAP_CYCLES: AtomicU32 = AtomicU32::new(0);
/// Gaps whose raw cycle delta was too wide to trust (see [`Producer::on_callback`]).
pub(crate) static INVALID_GAPS: AtomicU32 = AtomicU32::new(0);
/// Ring-fill builds only: `embassy_time` milliseconds at the first armed callback and at the
/// producer's disarm, which bracket the device's own measure of how long the ring took to fill.
/// Zero means "not yet", which no real stamp can be: arming waits [`ARM_DELAY_MS`](crate::geometry::ARM_DELAY_MS) past the first
/// callback. Milliseconds in a `u32` wrap after 49 days, and the single shot is over within
/// minutes of boot. DWT cannot time this: CYCCNT wraps every 8.9 s and the fill takes 349.5 s.
pub(crate) static FILL_STARTED_MS: AtomicU32 = AtomicU32::new(0);
pub(crate) static FILL_ENDED_MS: AtomicU32 = AtomicU32::new(0);
/// One-shot builds only: the block and callback whose input was being captured when the callback
/// rendered the stimulus's first sample. Written once, by that callback, before it sets
/// `STIM_STARTED` with release, so a reader that sees the flag sees both positions.
pub(crate) static STIM_STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) static STIM_BLOCK: AtomicU32 = AtomicU32::new(0);
pub(crate) static STIM_CALLBACK: AtomicU32 = AtomicU32::new(0);

/// Where one callback's input landed in the ring: block sequence number and callback within it.
#[derive(Clone, Copy)]
pub(crate) struct Captured {
    pub(crate) block: u32,
    pub(crate) callback: usize,
}

/// The audio callback's half of the capture: timing and the copy into the ring.
///
/// Everything it does to the world is an SDRAM write or an atomic store. No lock, no allocation, no
/// logging call, no await - a log call here would take the record lock with SAI1 masked behind it,
/// and an await would hand the deterministic SAI1 wake to the pender.
pub(crate) struct Producer {
    ring: Ring,
    /// Callbacks already copied into the block being filled; 0 means between blocks.
    callbacks_in_block: usize,
    /// DWT count at the previous callback's entry, `None` before the first.
    last_entry: Option<u32>,
}

impl Producer {
    pub(crate) fn new(ring: Ring) -> Self {
        Self {
            ring,
            callbacks_in_block: 0,
            last_entry: None,
        }
    }

    /// Record one callback: its entry time, then, if a capture is running, its input lane. Returns
    /// where that input landed, or `None` when nothing was captured.
    ///
    /// Called first thing in the callback with the cycle count taken on entry; the matching exit
    /// time goes to [`Producer::finish`].
    pub(crate) fn on_callback(&mut self, entry: u32, input: &[u32]) -> Option<Captured> {
        AUDIO_STARTED.store(true, Ordering::Relaxed);

        // CYCCNT is 32 bits and wraps every 8.9 s at 480 MHz, so a gap wider than half its range
        // is indistinguishable from a short one after the wrap. A delta at or past 2^31 cycles
        // (4.47 s) is therefore counted as invalid and kept out of `worst_gap`, rather than
        // aliased into a small number that would read as a healthy gap - the opposite of what a
        // multi-second stall means. `delivered` falling behind `expected` still reports the stall.
        if let Some(last) = self.last_entry {
            let gap = entry.wrapping_sub(last);
            if gap >= 1 << 31 {
                INVALID_GAPS.fetch_add(1, Ordering::Relaxed);
            } else {
                WORST_GAP_CYCLES.fetch_max(gap, Ordering::Relaxed);
            }
        }
        self.last_entry = Some(entry);

        if self.callbacks_in_block == 0 && !self.claim() {
            return None;
        }

        let captured = Captured {
            block: PRODUCED.load(Ordering::Relaxed),
            callback: self.callbacks_in_block,
        };
        let index = captured.block as usize % capture::RING_BLOCKS;
        let at = self.callbacks_in_block * capture::CALLBACK_BYTES;
        // Lane truncation into a stack copy, then one contiguous copy into SDRAM. The codec delivers
        // 32-bit left-justified PCM, so the top half-word is the `i16` sample: truncation, not
        // rescaling, and exact for anything the ADC resolves above 16 bits.
        let mut lane = [0u8; capture::CALLBACK_BYTES];
        for (bytes, words) in lane.chunks_exact_mut(2).zip(input.chunks_exact(2)) {
            let sample = (words[MONO_WORD] >> 16) as u16 as i16;
            bytes.copy_from_slice(&sample.to_le_bytes());
        }
        // Safety: this block is `Filling`, which only the producer can be.
        let block = unsafe { self.ring.block(index) };
        block[at..at + capture::CALLBACK_BYTES].copy_from_slice(&lane);

        self.callbacks_in_block += 1;
        if self.callbacks_in_block == capture::callbacks_per_block() {
            // Publish: the release in `advance` orders every byte above before the state.
            let published = advance(index, BlockState::Filling, BlockState::Full);
            debug_assert!(
                published,
                "a block the producer was filling changed state under it"
            );
            PRODUCED.fetch_add(1, Ordering::Release);
            FILLING.store(false, Ordering::Release);
            self.callbacks_in_block = 0;
        }
        Some(captured)
    }

    /// At a block boundary: claim the next block in ring order if a capture is armed.
    ///
    /// An overrun - the next block is not `Free` - leaves that block exactly as it is and disarms.
    /// Overwriting a block the writer has not shipped would turn a counted loss into silent
    /// corruption; stopping turns it into `overrun` plus a frozen tail the dump still delivers.
    ///
    /// In a ring-fill build that overrun is the window's planned end, and the two ends of the fill
    /// are stamped here, in the callbacks that start and stop it, rather than by the timeline's
    /// 100 ms poll. Reading `embassy_time` is a timer-register read and an atomic load, no lock.
    fn claim(&mut self) -> bool {
        if !ARMED.load(Ordering::Acquire) {
            return false;
        }
        let index = PRODUCED.load(Ordering::Relaxed) as usize % capture::RING_BLOCKS;
        if !advance(index, BlockState::Free, BlockState::Filling) {
            if RING_FILL {
                // Before the disarm's release, so the timeline that sees ARMED clear sees this too.
                FILL_ENDED_MS.store(now_ms(), Ordering::Relaxed);
            }
            OVERRUN.fetch_add(1, Ordering::Relaxed);
            ARMED.store(false, Ordering::Release);
            return false;
        }
        if RING_FILL && FILL_STARTED_MS.load(Ordering::Relaxed) == 0 {
            FILL_STARTED_MS.store(now_ms(), Ordering::Relaxed);
        }
        FILLING.store(true, Ordering::Release);
        true
    }

    /// Record how long the callback that started at `entry` ran.
    pub(crate) fn finish(&self, entry: u32) {
        let ran = DWT::cycle_count().wrapping_sub(entry);
        MAX_BLOCK_CYCLES.fetch_max(ran, Ordering::Relaxed);
    }
}
