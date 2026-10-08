//! Measurement-rig binary: plays a known stimulus, captures one lane, and hands the bytes to
//! the host for objective measurement (TASK-038).
//!
//! This file is deliberately a near-copy of `main.rs`. Everything it keeps - the preamble, the
//! board setup, the codec start, the block geometry, the encode step - behaves exactly as it
//! does in the binary already proven on hardware, which is the only reason to trust the parts
//! that are new. Three things differ, and they are the whole point:
//!
//! 1. `cortex_m::Peripherals::take()` is claimed, because rig owns DWT's cycle counter and
//!    measures durations with it (a duration measured against a counter nobody enabled reads
//!    zero and looks like a fast device).
//! 2. The audio future runs on an interrupt-mode executor woken by SAI1, not on the thread
//!    executor's select tree, so its latency has a bound rather than a hope.
//! 3. The DSP chain is a stimulus generator chosen by cargo feature, and the input frame never
//!    reaches it: rig measures the box, not the instrument plugged into it.
//!
//! What it then does with the input is the measurement. One lane of the codec's input is truncated
//! to `i16` and copied into a 32 MiB ring in SDRAM, the ring is shipped to the host over the framed
//! console once the window closes, and the timing of every callback is measured against DWT so the
//! host can tell a clean capture from a starved one. The run is a fixed timeline with no inbound
//! control: see "Capture timeline" below.

#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use asperitas_dsp::processor::{Frame, Processor};
#[cfg(feature = "stim-ess")]
use asperitas_dsp::stimulus::ExponentialSweep;
#[cfg(feature = "stim-pulse")]
use asperitas_dsp::stimulus::PulseTrain;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse")))]
use asperitas_dsp::stimulus::Sine;
use asperitas_dsp::stimulus::Stimulus;
use asperitas_logging::capture::{self, BlockState};
use asperitas_logging::{console, error, info};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use cortex_m::peripheral::{DWT, NVIC};
use daisy_embassy::audio::{AudioConfig, Fs, Interface, Running, BLOCK_LENGTH};
use daisy_embassy::hal::interrupt;
use daisy_embassy::hal::interrupt::{InterruptExt, Priority};
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use embassy_executor::{InterruptExecutor, Spawner};
use static_cell::StaticCell;

// ---------------------------------------------------------------------------
// Per-binary preamble
// ---------------------------------------------------------------------------
//
// Duplicated by hand in every binary here and unable to move into a lib crate: the
// `#[panic_handler]` attribute is only seen by the linker when it sits in the final binary
// crate, and `#[defmt::global_logger]` emits its symbols the same way (see `main.rs`'s note on
// dead-code elimination). Copying is the cost of that rule, not an oversight.

// Re-export the panic handler from asperitas-logging.
// The #[panic_handler] attribute must be in the binary crate for the linker
// to pick it up, so we define a thin wrapper here.
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    asperitas_logging::panic_handler::handle_panic(info)
}

// Provide _defmt_panic symbol required by embassy-stm32 / embassy-usb's
// internal defmt usage (defmt 1.x). This is NOT the Rust panic handler;
// it fires only when a defmt formatter encounters an unrecoverable error.
// No `bkpt()`: with no debug probe attached it escalates to a HardFault
// instead of halting, turning a diagnostic into a silent lockup.
// Stays compiled in under `log-defmt` too: `_defmt_panic` is its own symbol (defmt
// src/export/mod.rs) that defmt-rtt does *not* provide, so dropping this would break linking of
// every `defmt::assert!` in embassy-stm32. It is unrelated to the `#[panic_handler]` above.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// The defmt logger. Exactly one of these two is compiled in, and both are load-bearing: they
// supply `_defmt_write`, `_defmt_acquire`, `_defmt_release` and `_defmt_flush`, the symbols every
// defmt frame inside embassy-stm32 and daisy-embassy resolves against. Drop either half and the
// link fails on binaries that contain no defmt call of their own.
//
// With a probe wired, `defmt-rtt` fills its RTT ring and probe-rs reads it. Without one there is
// nobody to scan RAM, so the stub discards every byte - silent, but still required.
#[cfg(feature = "log-defmt")]
use defmt_rtt as _;

// No-op defmt logger.
//
// NOTE: This block must live in each binary crate, not in a shared lib.
// `#[defmt::global_logger]` is a proc-macro that emits linker symbols only
// when expanded inside the final binary crate; placing it in a lib crate
// causes dead-code elimination to drop the struct (and its generated
// symbols) because nothing references `Logger` by name.
#[cfg(not(feature = "log-defmt"))]
#[defmt::global_logger]
struct Logger;

#[cfg(not(feature = "log-defmt"))]
unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn release() {}
    unsafe fn flush() {}
    unsafe fn write(data: &[u8]) {
        let _ = data;
    }
}

// rig's own USB IRQ struct. Each binary defines one: `bind_interrupts!` expands to an
// `#[export_name]` thunk per vector, so sharing the struct across crates would mean sharing its
// handlers with binaries that may not even enable the console.
//
// SAI1 is deliberately absent from this struct - see the handler written by hand below.
bind_interrupts!(pub struct RigUsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// ---------------------------------------------------------------------------
// Stimulus selection
// ---------------------------------------------------------------------------

/// How many of the three generators a build selected. Zero is a valid answer and means sine.
const GENERATORS_SELECTED: usize = {
    // Written as a const block rather than `usize::from(cfg!(..))` sums: `From<bool>` is not
    // const-callable on this toolchain, so the conversion has to happen in plain control flow.
    let mut selected = 0;
    if cfg!(feature = "stim-sine") {
        selected += 1;
    }
    if cfg!(feature = "stim-ess") {
        selected += 1;
    }
    if cfg!(feature = "stim-pulse") {
        selected += 1;
    }
    selected
};

/// Two generators in one image is a mistake, not a mode: there is no inbound console channel
/// that could say which one the capture used, so the bytes would arrive describable only by
/// guessing. Reject it where the build can still hear about it.
///
/// This is also why `stim-sine` is not in `default`. A default sine plus `--features
/// seed3,stim-ess` would select two generators and trip this very assert.
///
/// Written `< 2` rather than `<= 1`: in a build that selects no generator the count is 0, and
/// clippy reads a comparison against 1 as one against the type's minimum and calls it vacuous
/// (`absurd_extreme_comparisons`). Same bound, and the count is still what a human should read.
const _: () = assert!(
    GENERATORS_SELECTED < 2,
    "select at most one stimulus: stim-sine, stim-ess or stim-pulse"
);

/// The generator this build plays.
///
/// The sine arm's `cfg` covers both spellings of "sine": explicitly `--features stim-sine`, and
/// no stimulus feature at all. Mutual exclusion above is what makes leaving `stim-sine` out of
/// that condition correct rather than a subtle wrong-build generator.
#[cfg(feature = "stim-ess")]
type ActiveGenerator = ExponentialSweep;
#[cfg(feature = "stim-pulse")]
type ActiveGenerator = PulseTrain;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse")))]
type ActiveGenerator = Sine;

/// Sample rate rig renders at, derived from the capture format rather than restated beside it.
///
/// The capture geometry in `asperitas-logging::capture` prices itself at 48 kHz mono 16-bit, and the
/// codec runs at whatever [`RIG_FS`] says. If those ever disagree the measurement is nonsense, so
/// one of them has to be the source of truth; the capture format is the one the host parses, and
/// the assert under [`RIG_FS`] holds the codec to it.
const SAMPLE_RATE_HZ: f32 = capture::SAMPLE_RATE_HZ as f32;

/// The codec rate, named rather than left to `AudioConfig::default()` so the build can check it.
const RIG_FS: Fs = Fs::Fs48000;

/// `Fs` as hertz. daisy-embassy keeps its own table private inside `into_clock_divider`, which also
/// reads the live SAI clock and so cannot run in const context; this is that table's first half.
const fn fs_hz(fs: Fs) -> u32 {
    match fs {
        Fs::Fs8000 => 8_000,
        Fs::Fs32000 => 32_000,
        Fs::Fs44100 => 44_100,
        Fs::Fs48000 => 48_000,
        Fs::Fs88200 => 88_200,
        Fs::Fs96000 => 96_000,
    }
}

// `capture` copies two driver facts it cannot name without pulling the HAL into a host-tested crate.
// This is the side that can see both, so the loop closes here.
const _: () = assert!(
    fs_hz(RIG_FS) == capture::SAMPLE_RATE_HZ,
    "the codec rate and the capture format disagree"
);
// Compared in samples, deliberately. `capture::CALLBACK_BYTES` (64 bytes of one mono i16 lane) and
// `daisy_embassy::audio::HALF_DMA_BUFFER_LENGTH` (64 u32 words of interleaved stereo) are equal as
// numbers and unrelated as quantities: one is what the ring gains per callback, the other is what
// the DMA moves. A gate written against those two would keep passing after either side changed
// shape, so it is written against the one quantity both sides mean the same thing by.
const _: () = assert!(
    capture::FRAMES_PER_CALLBACK == BLOCK_LENGTH,
    "capture::FRAMES_PER_CALLBACK no longer matches the driver's samples per callback"
);

/// Storage for the one generator instance.
static GENERATOR: StaticCell<ActiveGenerator> = StaticCell::new();

// ---------------------------------------------------------------------------
// Audio executor
// ---------------------------------------------------------------------------

/// Executor that owns the audio callback, driven by the SAI1 interrupt.
///
/// `InterruptExecutor::new()` is `const`, so a plain `static` is enough to give `start()` the
/// `&'static self` it wants; no `StaticCell` dance is needed for a value that can be built in
/// a `static` initializer.
static AUDIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

/// The SAI1 vector: poll the audio executor.
///
/// Hand-written, and that is the only form available. `bind_interrupts!` and `#[interrupt]` may
/// not both name a vector - the macro expands to `#[export_name = "SAI1"] fn __SAI1() {}`, so
/// binding SAI1 anywhere *and* defining this function is a duplicate-symbol link error. Nothing
/// binds SAI1 today: daisy-embassy's `AudioIrqs` binds only `DMA1_STREAM0`/`STREAM1` for
/// `DMA1_CH0`/`CH1` (`src/audio.rs:26-29`), and embassy-stm32's SAI driver binds only DMA lines,
/// so the vector is free and this definition is legal. Do not "tidy" it into a
/// `bind_interrupts!` block. The shape is upstream's: `examples/looper.rs:27-31` (this static and
/// vector) and `:132-134` (priority, `start`, spawn), daisy-embassy at the pinned `ca9bcc9`.
#[interrupt]
unsafe fn SAI1() {
    // Safety: called from the SAI1 handler and nowhere else, and only after `start()` has
    // initialised the executor - `start()` is what unmasks this vector, so no SAI1 interrupt can
    // reach here before then.
    unsafe { AUDIO_EXECUTOR.on_interrupt() }
}

/// Convert processed stereo frames back to u32 codec samples.
///
/// Values are clamped to [-1.0, 1.0] and scaled to 32-bit signed range. Copied from `main.rs`;
/// its `decode_block` sibling is absent here because rig ignores the input frame.
fn encode_block(input: &[Frame; BLOCK_LENGTH], output: &mut [u32]) {
    for (frame, words) in input.iter().zip(output.chunks_exact_mut(2)) {
        let left = (frame[0].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        let right = (frame[1].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        words[0] = left;
        words[1] = right;
    }
}

/// Emit one wire-contract body to the framed console, stamped with the clock as it is now.
///
/// Two definitions rather than a `cfg` at each call site, because there are many (RIGCFG, RIGGEN,
/// CAPMAX, CAPSTAT, DUMPEND) and that many copies of `#[cfg(feature = "log-usb")]` is that many
/// ways to forget one.
///
/// Under `log-defmt` without `log-usb` this discards, and that is honest rather than a gap: the
/// verbs describe the byte stream the framed console carries, and an RTT-only image captures no
/// such stream. The shim exists so that build still links (CI's second configuration) without
/// pretending a measurement record went somewhere.
#[cfg(feature = "log-usb")]
fn emit_console(body: &[u8]) {
    let now_ms = embassy_time::Instant::now().as_millis() as u32;
    if !asperitas_logging::emit_record(asperitas_logging::Level::Info, now_ms, body) {
        error!("rig: console refused a record (log pipe full)");
    }
}

#[cfg(not(feature = "log-usb"))]
fn emit_console(_body: &[u8]) {}

// ---------------------------------------------------------------------------
// Capture ring
// ---------------------------------------------------------------------------
//
// One producer (the audio callback at P6) and one consumer (the dump writer in thread mode) share
// the ring through a byte of state per block and nothing else. The producer claims a `Free` block,
// fills it, and publishes it `Full` with release ordering; the writer claims a `Full` block with
// acquire ordering, ships it, and hands it back `Free` with release ordering. Payload is always
// written before the state byte that publishes it, so whoever observes the new state also observes
// the bytes. That is the classic SPSC discipline, and it is why this section does not reuse
// `main.rs`'s `UnsafeCell` + `interrupt::free` idiom: a critical section is right for one slow knob
// writer and wrong for handing buffer ownership across an interrupt boundary 1 500 times a second.
//
// The ring is uncached by memory type, not by luck: no MPU region covers 0xC000_0000 (daisy-embassy
// puts its cacheable region at 0xD000_0000, an unconfigured FMC bank), so the default map makes the
// window Device memory, and enabling the D-cache does not touch it. That was measured off the
// running board on 2026-10-08 (docs/reference/daisy-seed3.md, "External SDRAM"). What would change
// this hand-off is an MPU region over 0xC000_0000: whoever adds one owns the coherence argument for
// the FMC window and revisits these orderings in the same change. Device memory also takes no
// unaligned accesses, so the ring is only ever touched through aligned copies.

/// Which input lane the ring keeps.
///
/// Left, matching `main.rs`'s `decode_block`, which reads `words[0]` as left from the same
/// interleaved layout. Which physical jack that is on the Pod is TASK-038.05's observation to make
/// at the bench; if it is the wrong one, flipping this constant is the whole fix, because
/// [`MONO_WORD`] and `RIGCFG`'s `lane` both derive from it.
const MONO_LANE: console::MonoLane = console::MonoLane::Left;

/// The interleaved word offset [`MONO_LANE`] names.
const MONO_WORD: usize = match MONO_LANE {
    console::MonoLane::Left => 0,
    console::MonoLane::Right => 1,
};

/// Seconds of capture this build records, from `ASP_RIG_CAPTURE_SECONDS` when set.
///
/// The override exists so a bench run can be shortened without editing source
/// (`ASP_RIG_CAPTURE_SECONDS=30 cargo build ...`); rustc tracks the variable, so changing it
/// rebuilds. Whatever it says is held to the ring gate below, so an override can shorten the window
/// freely and lengthen it only as far as the ring allows.
const CAPTURE_SECONDS: usize = match option_env!("ASP_RIG_CAPTURE_SECONDS") {
    Some(text) => parse_seconds(text),
    None => capture::CAPTURE_WINDOW_SECONDS,
};

/// Parse a whole number of seconds at compile time. A malformed value fails the build, naming the
/// variable, instead of falling back to a default nobody asked for.
const fn parse_seconds(text: &str) -> usize {
    let bytes = text.as_bytes();
    assert!(
        !bytes.is_empty(),
        "ASP_RIG_CAPTURE_SECONDS is set but empty"
    );
    let mut seconds = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i].is_ascii_digit(),
            "ASP_RIG_CAPTURE_SECONDS must be a whole number of seconds"
        );
        seconds = seconds * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    seconds
}

/// Ring blocks the window consumes: 879 at the default 300 s. Rounded up by `expected_blocks`,
/// because the producer always finishes the block it is in when the window closes.
const WINDOW_BLOCKS: usize = capture::expected_blocks(CAPTURE_SECONDS);

/// One audio period in whole microseconds, floored: 666 (32 frames at 48 kHz is 666.67 us).
///
/// Floored because it is used as a deadline, and a deadline rounded up admits a callback that
/// actually missed it.
const PERIOD_US: u32 =
    (capture::FRAMES_PER_CALLBACK as u64 * 1_000_000 / capture::SAMPLE_RATE_HZ as u64) as u32;

/// Longest a callback may run: one period. Past it the callback is still working when the DMA
/// wants the next half-buffer, which is the definition of an underrun rather than a slow one.
const CALLBACK_BUDGET_US: u32 = PERIOD_US;

/// Longest gap between callback starts that still counts as on time: one period plus a quarter.
///
/// The slack absorbs interrupt-entry jitter - callback N entered late, N+1 on time - which stretches
/// one gap and shrinks the next without losing a sample. A quarter period is far more jitter than
/// the P6 executor should ever see and far less than a missed callback, which is what the gate
/// below pins.
const GAP_LIMIT_US: u32 = PERIOD_US + PERIOD_US / 4;

// Rate gates. The geometric ones are compile-time facts and fail the build. The three that judge a
// measurement (`worst_gap_us`, `max_block_us`, delivered against expected) can only be judged once a
// window has run; `judge_capture` reports each beside the number it judges, and the asserts here pin
// the thresholds those judgments use so they cannot drift into meaninglessness.
const _: () = assert!(
    CAPTURE_SECONDS > 0,
    "a zero-second capture window records nothing"
);
const _: () = assert!(
    WINDOW_BLOCKS < capture::RING_BLOCKS,
    "the capture window does not fit the ring; shorten ASP_RIG_CAPTURE_SECONDS"
);
const _: () = assert!(
    capture::RING_BYTES <= daisy_embassy::sdram::SDRAM_SIZE,
    "the capture ring is larger than the SDRAM it lives in"
);
const _: () = assert!(
    GAP_LIMIT_US > PERIOD_US && GAP_LIMIT_US < 2 * PERIOD_US,
    "the gap limit must admit jitter and still catch one missed callback"
);
const _: () = assert!(
    CALLBACK_BUDGET_US <= PERIOD_US,
    "a callback budget past one period admits underruns"
);

/// How often `report_capstat` emits, gated below against the dump it shares the pipe with.
const CAPSTAT_PERIOD_MS: usize = 1_000;

/// Milliseconds one ring block takes to fill: 341.
const BLOCK_FILL_MS: usize = capture::callbacks_per_block() * capture::FRAMES_PER_CALLBACK * 1_000
    / capture::SAMPLE_RATE_HZ as usize;

/// Most `CAPSTAT` records that can land while one block fills, plus one for phase slip: 2.
const CAPSTAT_MAX_PER_BLOCK: usize = BLOCK_FILL_MS / CAPSTAT_PERIOD_MS + 2;

// Status traffic cannot dominate the dump: worst-case CAPSTAT bytes per block stay under one
// percent of the wire bytes that block costs to dump (2 x 200 x 100 < 57 788). Relative on
// purpose, because the link ceiling is unmeasured; both sides are constants the crate that renders
// them owns and host-tests, not numbers typed here.
const _: () = assert!(
    CAPSTAT_MAX_PER_BLOCK * console::CAPSTAT_MAX_BODY * 100 < capture::wire_bytes_per_block(),
    "CAPSTAT traffic would exceed one percent of the dump's own record traffic"
);

/// Delay from the first audio callback to arming.
///
/// Long enough for `BOOT`, `RIGCFG`, `RIGGEN` and `CAPMAX` to have drained to an attached host, so
/// the description of a capture always precedes it, and short enough not to matter to a person
/// waiting at the bench.
const ARM_DELAY_MS: u64 = 2_000;

/// How often the capture timeline re-checks for an early stop.
const CONTROL_POLL_MS: u64 = 100;

/// How long the dump writer yields after a refused chunk. Short, because a refusal clears as soon
/// as the drain has moved one packet; the point of the wait is the yield, not the duration.
#[cfg(feature = "log-usb")]
const DUMP_RETRY_MS: u64 = 1;

/// Base address of the ring in SDRAM, as handed back by `Sdram::init`.
///
/// Carried as a raw pointer rather than a slice on purpose: producer and writer hold different
/// blocks of the same 32 MiB at the same time, and two `&mut [u8]` over one allocation would be
/// aliasing whatever the block states say. Each side builds a slice over exactly one block, only
/// after the state byte has made that block its own.
#[derive(Clone, Copy)]
struct Ring {
    base: *mut u8,
}

// Safety: the pointer names memory-mapped SDRAM that lives for the program's lifetime, and every
// access through it is gated by `BLOCK_STATE`, which hands each block to exactly one side.
unsafe impl Send for Ring {}

impl Ring {
    /// The bytes of block `index`.
    ///
    /// # Safety
    /// The caller must own the block through [`BLOCK_STATE`] - `Filling` for the producer,
    /// `Dumping` for the writer - for as long as the slice lives.
    #[allow(clippy::mut_from_ref)] // Ownership comes from the state byte, not from `&self`.
    unsafe fn block(&self, index: usize) -> &mut [u8] {
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
static BLOCK_STATE: [AtomicU8; capture::RING_BLOCKS] =
    [const { AtomicU8::new(BlockState::Free.as_u8()) }; capture::RING_BLOCKS];

/// Move block `index` from `from` to `to`, if it is in `from`. Returns whether it was.
///
/// The only way any block state changes, so the edge check against `capture::transition_ok` sits in
/// one place and covers producer and writer alike. Acquire on success makes the previous owner's
/// writes visible before this side touches the block; release makes this side's writes visible to
/// the next owner.
fn advance(index: usize, from: BlockState, to: BlockState) -> bool {
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
static PRODUCED: AtomicU32 = AtomicU32::new(0);
/// Blocks the producer refused to claim because the next one in ring order was not `Free`. A
/// count of refusals, not of bytes: the first one stops the capture, so it reads 0 or 1 per run.
static OVERRUN: AtomicU32 = AtomicU32::new(0);
/// Blocks whose `AUDEND` has been committed to the console.
static DUMPED: AtomicU32 = AtomicU32::new(0);
/// Set by the capture timeline to start a capture, cleared by it at the deadline and by the
/// producer on overrun. The producer reads it only at a block boundary.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Whether the producer currently owns a `Filling` block.
static FILLING: AtomicBool = AtomicBool::new(false);
/// Set by the first audio callback.
static AUDIO_STARTED: AtomicBool = AtomicBool::new(false);
/// `CAPSTAT`'s `audio_exit`: 0 running, 1 `start_interface` failed, 2 `start_callback` failed.
static AUDIO_EXIT: AtomicU32 = AtomicU32::new(0);
/// Longest callback and longest gap between callback starts, in raw DWT cycles. Converted to
/// microseconds only when reported, with the time base boot settled on.
static MAX_BLOCK_CYCLES: AtomicU32 = AtomicU32::new(0);
static WORST_GAP_CYCLES: AtomicU32 = AtomicU32::new(0);
/// Gaps whose raw cycle delta was too wide to trust (see [`Producer::on_callback`]).
static INVALID_GAPS: AtomicU32 = AtomicU32::new(0);

/// The audio callback's half of the capture: timing and the copy into the ring.
///
/// Everything it does to the world is an SDRAM write or an atomic store. No lock, no allocation, no
/// logging call, no await - a log call here would take the record lock with SAI1 masked behind it,
/// and an await would hand the deterministic SAI1 wake to the pender.
struct Producer {
    ring: Ring,
    /// Callbacks already copied into the block being filled; 0 means between blocks.
    callbacks_in_block: usize,
    /// DWT count at the previous callback's entry, `None` before the first.
    last_entry: Option<u32>,
}

impl Producer {
    /// Record one callback: its entry time, then, if a capture is running, its input lane.
    ///
    /// Called first thing in the callback with the cycle count taken on entry; the matching exit
    /// time goes to [`Producer::finish`].
    fn on_callback(&mut self, entry: u32, input: &[u32]) {
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
            return;
        }

        let index = PRODUCED.load(Ordering::Relaxed) as usize % capture::RING_BLOCKS;
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
    }

    /// At a block boundary: claim the next block in ring order if a capture is armed.
    ///
    /// An overrun - the next block is not `Free` - leaves that block exactly as it is and disarms.
    /// Overwriting a block the writer has not shipped would turn a counted loss into silent
    /// corruption; stopping turns it into `overrun` plus a frozen tail the dump still delivers.
    fn claim(&mut self) -> bool {
        if !ARMED.load(Ordering::Acquire) {
            return false;
        }
        let index = PRODUCED.load(Ordering::Relaxed) as usize % capture::RING_BLOCKS;
        if !advance(index, BlockState::Free, BlockState::Filling) {
            OVERRUN.fetch_add(1, Ordering::Relaxed);
            ARMED.store(false, Ordering::Release);
            return false;
        }
        FILLING.store(true, Ordering::Release);
        true
    }

    /// Record how long the callback that started at `entry` ran.
    fn finish(&self, entry: u32) {
        let ran = DWT::cycle_count().wrapping_sub(entry);
        MAX_BLOCK_CYCLES.fetch_max(ran, Ordering::Relaxed);
    }
}

/// Raw cycles to whole microseconds, with the time base boot settled on.
fn cycles_to_us(cycles: u32, time_base: TimeBase) -> u32 {
    cycles / time_base.cycles_per_us
}

/// The one `CAPMAX` record: what this build's ring could hold, all from `capture::` arithmetic.
fn emit_capmax() {
    let window_bytes = CAPTURE_SECONDS * capture::BYTES_PER_SECOND;
    let cap = console::RingCapacity {
        total_bytes: window_bytes as u32,
        ring_bytes: capture::RING_BYTES as u32,
        seconds_max: capture::ring_seconds_floor(),
        us_max: capture::ring_duration_micros() as u32,
        // Block-granular, because the ring is consumed in whole blocks: these are the bytes no
        // block of this window will ever touch.
        unused_headroom_bytes: ((capture::RING_BLOCKS - WINDOW_BLOCKS) * capture::RING_BLOCK_BYTES)
            as u32,
    };
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::capmax_body(&cap, &mut body);
    emit_console(&body[..n]);
}

/// The capture counters as they stand, in `CAPSTAT`'s shape.
fn capture_status(time_base: TimeBase) -> console::CaptureStatus {
    let delivered = PRODUCED.load(Ordering::Acquire);
    console::CaptureStatus {
        delivered,
        // Counting down, as the field is defined: blocks this window still owes. Zero at the end of
        // a window that delivered everything.
        expected: (WINDOW_BLOCKS as u32).saturating_sub(delivered),
        overrun: OVERRUN.load(Ordering::Relaxed),
        max_block_us: cycles_to_us(MAX_BLOCK_CYCLES.load(Ordering::Relaxed), time_base),
        worst_gap_us: cycles_to_us(WORST_GAP_CYCLES.load(Ordering::Relaxed), time_base),
        audio_exit: AUDIO_EXIT.load(Ordering::Relaxed),
        dumped: DUMPED.load(Ordering::Relaxed),
        dropped_full: console::CONSOLE.snapshot().dropped_full,
    }
}

/// Emit `CAPSTAT` about once a second, forever, reading atomics only.
///
/// Cheap on purpose: it shares the log pipe with the dump, and a dump only ever gets what the
/// reserve leaves. If this cadence ever crowds the dump, `DUMPEND`'s `dropped_full` and `refused`
/// climb, and the period here is the first knob to turn.
async fn report_capstat(time_base: TimeBase) {
    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_millis(
        CAPSTAT_PERIOD_MS as u64,
    ));
    #[cfg(feature = "log-usb")]
    let mut body = [0u8; console::BODY_WINDOW];
    loop {
        ticker.next().await;
        let st = capture_status(time_base);
        #[cfg(feature = "log-usb")]
        {
            let n = console::capstat_body(&st, &mut body);
            emit_console(&body[..n]);
        }
        // No console means no CAPSTAT record, but the starvation signals are still worth a probe
        // reader's time, so the same facts go out as an ordinary log line.
        #[cfg(not(feature = "log-usb"))]
        info!(
            "rig: capstat delivered={} expected={} overrun={} max_block_us={} worst_gap_us={} audio_exit={} dumped={} dropped_full={}",
            st.delivered,
            st.expected,
            st.overrun,
            st.max_block_us,
            st.worst_gap_us,
            st.audio_exit,
            st.dumped,
            st.dropped_full,
        );
    }
}

/// Judge the window that just closed against the runtime half of the rate gates, each beside the
/// number it judges.
fn judge_capture(time_base: TimeBase, first_block: u32) {
    let st = capture_status(time_base);
    let delivered = st.delivered - first_block;
    let verdict = |ok: bool| if ok { "pass" } else { "FAIL" };
    info!(
        "rig: gate delivered {delivered} == expected {WINDOW_BLOCKS}: {}",
        verdict(delivered as usize == WINDOW_BLOCKS)
    );
    info!(
        "rig: gate worst_gap_us {} < {GAP_LIMIT_US}: {} ({} gaps too wide to measure)",
        st.worst_gap_us,
        verdict(st.worst_gap_us < GAP_LIMIT_US),
        INVALID_GAPS.load(Ordering::Relaxed),
    );
    info!(
        "rig: gate max_block_us {} < {CALLBACK_BUDGET_US}: {}",
        st.max_block_us,
        verdict(st.max_block_us < CALLBACK_BUDGET_US)
    );
    info!(
        "rig: gate overrun {} == 0: {}",
        st.overrun,
        verdict(st.overrun == 0)
    );
}

// ---------------------------------------------------------------------------
// Capture timeline
// ---------------------------------------------------------------------------
//
// rig arms itself. There is no inbound console channel to arm it through (runtime control is
// TASK-032), so the run is a fixed timeline, repeated by resetting the board, which `slow-boot`
// keeps safe for DFU:
//
//   boot      BOOT, RIGCFG, RIGGEN, CAPMAX, then audio starts
//   +2 s      after the first callback: arm (ARM_DELAY_MS)
//   +window   disarm; the producer finishes the block it is in, so exactly WINDOW_BLOCKS land
//             - or earlier, if the ring fills and the producer disarms itself on overrun
//   then      judge the window, dump every captured block in ring order, emit DUMPEND
//   after     idle; CAPSTAT keeps reporting once a second
//
// The dump waits for the window to close rather than draining alongside it so the measured window
// carries no bulk USB traffic: worst_gap_us and max_block_us then describe the audio path alone. The
// gates guarantee the window fits the ring, so nothing is lost by waiting.
//
// Single-shot looks like a missing feature. It is the absence of an inbound channel, stated here so
// nobody adds a re-arm path that nothing can trigger.

/// Run the timeline above once, then idle.
async fn run_capture(ring: Ring, time_base: TimeBase) {
    while !AUDIO_STARTED.load(Ordering::Relaxed) {
        if AUDIO_EXIT.load(Ordering::Relaxed) != 0 {
            error!("rig: audio never started; nothing to capture");
            return;
        }
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }
    embassy_time::Timer::after_millis(ARM_DELAY_MS).await;

    let first_block = PRODUCED.load(Ordering::Acquire);
    info!("rig: capture armed for {CAPTURE_SECONDS} s ({WINDOW_BLOCKS} blocks)");
    ARMED.store(true, Ordering::Release);

    let deadline =
        embassy_time::Instant::now() + embassy_time::Duration::from_secs(CAPTURE_SECONDS as u64);
    while ARMED.load(Ordering::Acquire) && embassy_time::Instant::now() < deadline {
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }
    ARMED.store(false, Ordering::Release);
    // The producer checks ARMED only at a block boundary, and it runs at P6 above this code, so
    // after the store above it can start no new block: waiting for FILLING to clear waits for the
    // one block in flight, at most 341 ms.
    while FILLING.load(Ordering::Acquire) {
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }

    let end_block = PRODUCED.load(Ordering::Acquire);
    info!(
        "rig: capture closed with {} blocks",
        end_block - first_block
    );
    judge_capture(time_base, first_block);
    dump_ring(ring, first_block, end_block).await;
    info!("rig: run complete; reset to capture again");
}

// ---------------------------------------------------------------------------
// Dump writer
// ---------------------------------------------------------------------------

/// Running totals for one dump, which become its `DUMPEND`.
#[cfg(feature = "log-usb")]
#[derive(Default)]
struct DumpTally {
    chunks: u32,
    bytes: u32,
    refused: u32,
    stall_ms: u32,
}

/// Commit one dump body, waiting out refusals.
///
/// `try_emit_dump` is the only door, and behind it `dump::dump_fits` keeps one maximum frame of
/// the pipe free for log and `STATUS` records, so a saturated dump cannot crowd them out (the
/// behaviour `tests/console_dump.rs::log_records_survive_a_saturated_dump` protects). A refusal
/// means the pipe is full of earlier chunks; the await between retries is load-bearing, because
/// the USB drain task that frees that space runs on this same thread executor and gets nowhere
/// while this loop holds it.
#[cfg(feature = "log-usb")]
async fn send_dump_body(body: &[u8], tally: &mut DumpTally) {
    let mut stalled_since: Option<embassy_time::Instant> = None;
    while !asperitas_logging::try_emit_dump(body) {
        tally.refused += 1;
        stalled_since.get_or_insert_with(embassy_time::Instant::now);
        embassy_time::Timer::after_millis(DUMP_RETRY_MS).await;
    }
    if let Some(since) = stalled_since {
        tally.stall_ms = tally.stall_ms.max(since.elapsed().as_millis() as u32);
    }
}

/// Ship blocks `first..end` (sequence numbers since boot) in ring order, then one `DUMPEND`.
///
/// Each block is claimed `Full -> Dumping`, sliced into `CHUNK_RAW` chunks with its CRC folded in
/// as each chunk is sliced - one pass over the bytes, not a second one that someone later
/// "optimises" by skipping - closed with `AUDEND`, and handed back `Free`. The block number on the
/// wire is the sequence number, so a host can tell this run's blocks apart from a previous one's.
#[cfg(feature = "log-usb")]
async fn dump_ring(ring: Ring, first: u32, end: u32) {
    use asperitas_logging::{dump, frame};

    let chunks = capture::chunks_per_block() as u16;
    let mut tally = DumpTally::default();
    let mut body = [0u8; frame::MAX_BODY];
    let mut blocks = 0u32;
    let started = embassy_time::Instant::now();

    info!("rig: dumping {} blocks", end - first);
    for seq in first..end {
        let index = seq as usize % capture::RING_BLOCKS;
        if !advance(index, BlockState::Full, BlockState::Dumping) {
            error!("rig: block {seq} was not Full at dump time; skipping it");
            continue;
        }
        // Safety: this block is `Dumping`, which only the writer can be.
        let block = unsafe { ring.block(index) };
        let mut crc = frame::CRC16_INITIAL;
        for (c, raw) in block.chunks(dump::CHUNK_RAW).enumerate() {
            crc = frame::crc16_ccitt_update(crc, raw);
            let n = match dump::audio_body(seq, chunks, c as u16, raw, &mut body) {
                Ok(n) => n,
                Err(e) => {
                    // Unreachable while `capture`'s geometry gates hold; refusing beats shipping a
                    // block the host would reassemble wrong.
                    error!(
                        "rig: AUDIO body for block {seq} chunk {c} refused: {e:?}; dump abandoned"
                    );
                    return;
                }
            };
            send_dump_body(&body[..n], &mut tally).await;
            tally.chunks += 1;
            tally.bytes += raw.len() as u32;
        }
        let n = match dump::audend_body(
            seq,
            chunks,
            capture::RING_BLOCK_BYTES as u32,
            crc,
            &mut body,
        ) {
            Ok(n) => n,
            Err(e) => {
                error!("rig: AUDEND body for block {seq} refused: {e:?}; dump abandoned");
                return;
            }
        };
        send_dump_body(&body[..n], &mut tally).await;
        advance(index, BlockState::Dumping, BlockState::Free);
        DUMPED.fetch_add(1, Ordering::Relaxed);
        blocks += 1;
    }

    let console_now = console::CONSOLE.snapshot();
    let summary = console::DumpSummary {
        blocks,
        chunks: tally.chunks,
        bytes: tally.bytes,
        elapsed_ms: started.elapsed().as_millis() as u32,
        refused: tally.refused,
        stall_ms: tally.stall_ms,
        sent: console_now.records_sent,
        dropped_full: console_now.dropped_full,
        bytes_dropped: console_now.bytes_dropped,
    };
    let mut record = [0u8; console::BODY_WINDOW];
    let n = console::dumpend_body(&summary, &mut record);
    emit_console(&record[..n]);
}

/// The RTT-only image has no framed console to dump over, so the capture stays in SDRAM.
///
/// Said once rather than pretended: a writer that "sent" every chunk into the discard shim would
/// end in a `DUMPEND` describing a transfer that never happened.
#[cfg(not(feature = "log-usb"))]
async fn dump_ring(_ring: Ring, first: u32, end: u32) {
    info!(
        "rig: {} blocks captured; this image has no framed console, so nothing is dumped",
        end - first
    );
}

/// The audio callback, running on [`AUDIO_EXECUTOR`] at SAI1's priority.
///
/// `generator` is a sole `&'static mut`: the only other handle was consumed by the move through
/// `spawn`, so the callback needs no lock. That is deliberate - the alternative, reaching the
/// generator through `cortex_m::interrupt::free`, puts a PRIMASK critical section on the
/// highest-rate path in the firmware, where the whole point of the interrupt executor is to
/// avoid one.
///
/// ### Timer-slot budget: 5 named users of 8 slots, and none of them here
///
/// `generic-queue-8` is fixed by daisy-embassy's own `embassy-time` dependency; adding a
/// `generic-queue-N` here would collide on `const QUEUE_SIZE` and fail to compile, so eight is
/// the whole budget. Its users: boot 1 (DWT calibration, the codec's 2 ms startup delay,
/// `slow-boot`'s linger), `CAPSTAT` reporting ticker 1, LED blink 1, capture timeline 1 (arm delay,
/// window deadline, early-stop poll), dump retry 1, USB drain 0 - deliberately zero, see `usb.rs`'s
/// parking drain. Five, as a ceiling. In practice fewer are held at once: slots are keyed by
/// *waker*, every one of those five runs inside `main`'s task (boot before the select, the rest
/// joined inside it), and so they collapse onto that task's single slot. Overflow does not panic:
/// `queue_generic.rs` evicts the furthest-out timer and wakes it early, so a blown budget shows up
/// as a mistimed `CAPSTAT` rather than an error. Count a new timer against the five above, and a
/// new *task* with a timer against the eight.
///
/// This task takes none of them: after `start_callback` is awaited there is nothing left to
/// await, which is the requirement rather than an observation. A task that awaited between blocks
/// would be re-spawned through the pender and turn the deterministic wake (SAI1 fired) into a
/// queueing delay.
#[embassy_executor::task]
async fn audio_task(
    mut interface: Interface<'static, Running>,
    generator: &'static mut ActiveGenerator,
    ring: Ring,
) {
    // Silence handed to the generator because `Processor` is shaped for effects: `tick` takes an
    // input frame and these sources discard it. The codec's input goes to the capture ring only,
    // never to the generator or the output.
    let frames_in = [Frame::default(); BLOCK_LENGTH];
    let mut frames_out = [Frame::default(); BLOCK_LENGTH];
    let mut producer = Producer {
        ring,
        callbacks_in_block: 0,
        last_entry: None,
    };

    let outcome = interface
        .start_callback(|input, output| {
            // Entry time first, exit time last, so `max_block_us` prices everything the callback
            // does, capture included.
            let entry = DWT::cycle_count();

            // Render site, mirroring `main.rs`: stimulus -> [processor slot] -> encode_block.
            // The processor slot is empty by design and is where a measured effect will hang.
            generator.process_block(&frames_in, &mut frames_out);
            encode_block(&frames_out, output);

            // Capture what came back: the input lane, after the self-loopback cable.
            producer.on_callback(entry, input);
            producer.finish(entry);
        })
        .await;

    // Reached only if SAI errors: `start_callback` returns `Result<Infallible, sai::Error>`, and
    // the `Ok` arm cannot be constructed.
    match outcome {
        Ok(never) => match never {},
        Err(_) => {
            AUDIO_EXIT.store(2, Ordering::Relaxed);
            error!("rig: audio callback stopped with an SAI error");
        }
    }
    // Return rather than halt. Nothing awaits after `start_callback`, so the task simply ends and
    // the audio executor idles; thread mode keeps running, and `CAPSTAT` then says
    // `audio_exit=2` once a second. Spinning here instead would block every priority below P6 -
    // thread mode included - and the one record that explains the silence would never be sent.
}

/// Emit the two description records - exactly one `RIGCFG`, then exactly one `RIGGEN`.
///
/// Emitted at boot, once the interface is running and immediately before the audio task is
/// spawned, because that is what `console::RigConfig`'s own contract says: "sent once at boot,
/// before any stimulus plays". The other shape available here - a flag captured by the callback,
/// rendered on its first block - buys nothing. Nothing between this call and the first callback can
/// change a parameter, and capture bytes go to the SDRAM ring rather than onto the console, so there
/// is no record ordering to win. What it would cost is real: rendering both bodies is core `fmt`
/// work, and core `fmt` takes longer over some values than others, which is precisely the kind of
/// work that does not belong on the first SAI1 interrupt of a run whose whole purpose is measuring
/// timing.
fn emit_descriptors(generator: &ActiveGenerator, time_base: TimeBase) {
    let mut body = [0u8; console::BODY_WINDOW];

    let cfg = console::RigConfig {
        lane: MONO_LANE,
        blocks: asperitas_logging::capture::RING_BLOCKS as u32,
        block_bytes: asperitas_logging::capture::RING_BLOCK_BYTES as u32,
        bytes_per_s: asperitas_logging::capture::BYTES_PER_SECOND as u32,
        capsec_us: asperitas_logging::capture::ring_duration_micros() as u32,
        window_s: CAPTURE_SECONDS as u32,
        cpu_hz: time_base.cpu_hz,
        icache: cortex_m::peripheral::SCB::icache_enabled(),
        dcache: cortex_m::peripheral::SCB::dcache_enabled(),
    };
    let n = console::rigcfg_body(&cfg, &mut body);
    emit_console(&body[..n]);

    // The invariant, stated where it can bite: these bytes must fit `RIGGEN_MAX_GEN_BYTES` (160).
    // Today's three generators at their `Default` parameters, described at 48 kHz, come to 88 bytes
    // for sine, 107 for the sweep, and 107 for the pulse train - the widest of the three - leaving
    // 53 bytes of slack. That margin is not kept by this comment: the worst-case check in
    // `crates/asperitas-dsp/tests/stimulus_tests.rs` pushes every field to its sanitized extreme and
    // asserts the result against the same 160, so a parameter change that eats the slack fails a
    // host test instead of arriving here as a clipped record.
    //
    // Which leaves what no host test can rule out: a future generator with fields nobody imagined.
    // So the length is measured against the constant rather than trusted. Describing into one byte
    // more than the verb will carry makes an overrun observable - `riggen_body` clips in place,
    // which would send `max_frequency_hz=8000.00` for a cut `8000.000000` and let the host divide
    // by a number that was never played. The spare byte turns that silent lie into a length a
    // release build can act on, which a `debug_assert!` cannot.
    let mut described = [0u8; console::RIGGEN_MAX_GEN_BYTES + 1];
    let n_desc = generator.describe(&mut described);
    if n_desc > console::RIGGEN_MAX_GEN_BYTES {
        error!(
            "rig: generator description is {n_desc} bytes, budget is {}; sending no RIGGEN rather than a clipped one",
            console::RIGGEN_MAX_GEN_BYTES
        );
        return;
    }
    let n = console::riggen_body(&described[..n_desc], &mut body);
    emit_console(&body[..n]);
}

// ---------------------------------------------------------------------------
// Boot: cycle counter and clock
// ---------------------------------------------------------------------------

/// DEMCR, the register `DCB::enable_trace()` sets TRCENA in.
///
/// cortex-m gives `DCB` a writer and no reader, so proving TRCENA took requires going at the
/// address directly. One volatile read of a control register; nothing aliases it.
const DEMCR: usize = 0xE000_EDFC;
const DEMCR_TRCENA: u32 = 1 << 24;

/// Length of the calibration window, and the tolerance beyond which the measured clock wins.
const CALIBRATION_MS: u64 = 200;
const CALIBRATION_TOLERANCE_PCT: u64 = 1;

/// Whether TRCENA is set, read from the hardware rather than inferred from the call that set it.
fn trace_enabled() -> bool {
    // Safety: DEMCR is a memory-mapped control register present on every Cortex-M7, and reading
    // it has no side effects.
    unsafe { core::ptr::read_volatile(DEMCR as *const u32) & DEMCR_TRCENA != 0 }
}

/// What rig divides cycle counts by, and how it knows.
#[derive(Clone, Copy)]
struct TimeBase {
    /// Core clock in Hz that rig converts with.
    cpu_hz: u32,
    /// Whole cycles per microsecond, rounded to nearest.
    ///
    /// What every reported duration is divided by (`cycles_to_us`); carried here from the one place
    /// that knows how the clock was settled rather than recomputed at each call site.
    cycles_per_us: u32,
}

/// Enable DWT's cycle counter, prove it counts, and settle on a cycles-per-microsecond.
///
/// Returns `None` when the counter cannot be made to run. Callers must treat that as fatal:
/// every duration rig reports is a cycle delta, and against a counter that stays at zero they
/// all read as zero, which is indistinguishable from a device fast enough to not matter.
///
/// `declared_hz` is the figure the clock tree computed from `default_rcc()`'s dividers. It agrees
/// with the datasheet maximum rather than with anything measured, and `rcc::clocks()` reports the
/// same derived number, so agreement between the two below is consistency, not confirmation.
async fn measure_time_base(
    cp: &mut cortex_m::peripheral::Peripherals,
    declared_hz: Option<u32>,
) -> Option<TimeBase> {
    if !cortex_m::peripheral::DWT::has_cycle_counter() {
        error!("rig: this core has no DWT cycle counter; refusing to report durations");
        return None;
    }

    cp.DCB.enable_trace(); // DEMCR.TRCENA: the CM7 may ignore CYCCNTENA without it.
    cortex_m::peripheral::DWT::unlock(); // LAR: the H7 locks the DWT after power-on.
    cp.DWT.enable_cycle_counter();
    // `PERIOD` is left alone on purpose: writing it switches the unit from a cycle counter to a
    // cycle *count* comparator, which would silently rescale every delta taken afterwards.

    // Readback of both enables, then liveness. The readback alone would pass on a counter that
    // accepted the enable bit and never advanced, so sample twice around a busy delay.
    let trcena = trace_enabled();
    let cyccntena = cortex_m::peripheral::DWT::cycle_counter_enabled();
    let before = cortex_m::peripheral::DWT::cycle_count();
    cortex_m::asm::delay(1000);
    let after = cortex_m::peripheral::DWT::cycle_count();

    if !(trcena && cyccntena) || before == after {
        error!(
            "rig: cycle counter unavailable (TRCENA={} CYCCNTENA={} count {} -> {})",
            u8::from(trcena),
            u8::from(cyccntena),
            before,
            after
        );
        return None;
    }

    // Independent cross-check: count cycles across a window the timer driver defines. If DWT were
    // frozen at some other rate - or fed a clock nobody intended - this is what notices.
    let start = embassy_time::Instant::now();
    let cal_before = cortex_m::peripheral::DWT::cycle_count();
    embassy_time::Timer::after_millis(CALIBRATION_MS).await;
    let elapsed_us = start.elapsed().as_micros();
    // Wrapping, because CYCCNT is 32 bits and wraps every 8.947 s at 480 MHz. A 200 ms window
    // cannot wrap; the subtraction is written so that it stays correct if this window grows.
    let cycles = cortex_m::peripheral::DWT::cycle_count().wrapping_sub(cal_before) as u64;
    if elapsed_us == 0 || cycles == 0 {
        error!("rig: calibration window produced nothing (us={elapsed_us}, cycles={cycles})");
        return None;
    }
    let measured_hz = (cycles * 1_000_000 / elapsed_us) as u32;

    // Publish the declared figure when the tree can state it in whole megahertz and the two agree
    // within a percent; otherwise fall back to the measurement and say so.
    //
    // Declared wins when it can, because the measurement is quantised by the tick rate
    // (`tick-hz-32_768`, one tick per 30.517 us). Over 200 ms that is +/- 0.015 %, which is
    // enough to throw away a digit of a 480 MHz quotient - 479 rather than 480, a 0.2 % error
    // paid on every duration rig reports, against a gate that only starts caring at 1 %.
    let declared_whole_mhz = declared_hz.filter(|hz| *hz % 1_000_000 == 0);
    let within_tolerance = |declared: u32| {
        u64::from(measured_hz)
            .max(u64::from(declared))
            .saturating_sub(u64::from(measured_hz).min(u64::from(declared)))
            * 100
            <= u64::from(declared) * CALIBRATION_TOLERANCE_PCT
    };

    let (cpu_hz, why) = match declared_whole_mhz {
        Some(declared) if within_tolerance(declared) => (declared, "declared, cross-checked"),
        Some(declared) => {
            error!(
                "rig: measured clock {measured_hz} Hz disagrees with declared {declared} Hz by over {CALIBRATION_TOLERANCE_PCT}%; using the measurement"
            );
            (measured_hz, "measured")
        }
        None => {
            info!("rig: declared clock unusable ({declared_hz:?}); using the measurement");
            (measured_hz, "measured")
        }
    };

    info!(
        "rig: time base {cpu_hz} Hz ({why}); declared {:?}, measured {measured_hz}; cycles/us {}; TRCENA={} CYCCNTENA={}",
        declared_hz,
        (u64::from(cpu_hz) + 500_000) / 1_000_000,
        u8::from(trcena),
        u8::from(cyccntena),
    );

    Some(TimeBase {
        cpu_hz,
        cycles_per_us: ((u64::from(cpu_hz) + 500_000) / 1_000_000) as u32,
    })
}

/// Stop rig in a state a person can see with no probe and no host attached.
fn boot_halt() -> ! {
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Panicked);
    #[allow(clippy::empty_loop)]
    loop {}
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Install the backend before the first record exists. Under `log-usb` this would be wrong: the
    // logger cannot be installed until the device is live, and [`asperitas_logging::usb::init`]
    // below does it, so this is only the other configurations' entry point.
    #[cfg(not(feature = "log-usb"))]
    asperitas_logging::init();

    info!("rig booting");

    // Claim the core peripherals BEFORE `hal::init`, so that whatever else in the stack wants
    // them finds them gone rather than half-configured by us. The `Option` is matched below
    // rather than unwrapped or `steal()`ed: `None` means somebody else owns the core peripherals,
    // and rig continuing anyway would report every duration as zero while looking healthy.
    let core_peripherals = cortex_m::Peripherals::take();

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);

    // Read the tree's own answer while the `Peri<RCC>` is still in hand - `new_daisy_board!`
    // moves the fields out of `p`, and `rcc::clocks()` is the accessor embassy-stm32 itself uses
    // internally for exactly this.
    let declared_hz = hal::rcc::clocks(&p.RCC).sys.to_hertz().map(|hz| hz.0);

    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals - usb::init() steals them directly via Peri::steal(). Same
    // invariant as `main.rs`: reading this field would create a second handle for the same
    // physical peripheral.
    let _ = board.usb_peripherals;

    // Init RGB LED - single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Init USB CDC serial logging.
    #[cfg(feature = "log-usb")]
    let _usb_handle = asperitas_logging::usb::init(RigUsbIrqs);
    #[cfg(feature = "log-usb")]
    info!("USB logging initialized");

    // The claim is matched here rather than at the `take()` above, and the gap is the LED: a
    // failure that early has no pin to report through, so taking the peripherals first and
    // reporting the consequence once the LED exists is what makes the failure visible.
    let mut cp = match core_peripherals {
        Some(cp) => cp,
        None => {
            error!(
                "rig: cortex_m::Peripherals was already claimed; refusing to guess at durations"
            );
            boot_halt();
        }
    };

    let time_base = match measure_time_base(&mut cp, declared_hz).await {
        Some(base) => base,
        None => boot_halt(),
    };

    // Bring up the SDRAM the capture ring lives in. `sdram` stays bound for the rest of `main`,
    // which never returns: it owns the FMC instance, and `init` takes it by `&mut`. Not because
    // dropping it would release the pins - `Sdram` has no `Drop` impl (daisy-embassy `ca9bcc9`) -
    // but because nothing else may ever touch that controller again.
    let mut sdram = board.sdram.build(&mut cp.MPU, &mut cp.SCB);
    let ring = Ring {
        base: sdram.init(&mut embassy_time::Delay).cast::<u8>(),
    };

    // Build the generator before audio starts. `ExponentialSweep`'s constructor peak-normalises
    // by scanning its whole 384,000-sample record, which is milliseconds at 48 kHz but belongs
    // here rather than in a callback under no circumstances. The log line goes first because a
    // board that died mid-scan would otherwise look idle.
    info!("rig: building stimulus (sweep builds scan their whole record)");
    let generator = GENERATOR.init(ActiveGenerator::default());
    generator.set_sample_rate(SAMPLE_RATE_HZ);

    // Prepare the audio interface (SAI + codec init + DMA buffers)
    let interface = board
        .audio_peripherals
        .prepare_interface(AudioConfig { fs: RIG_FS })
        .await;

    // Start SAI TX/RX and transition to Running state. A failure here does not halt: thread mode
    // still runs, and `CAPSTAT`'s `audio_exit=1` is the record that says why nothing is captured.
    // The LED says it too, for a bench with no host attached.
    let interface = match interface.start_interface().await {
        Ok(iface) => {
            info!("Audio interface ready");
            Some(iface)
        }
        Err(_) => {
            error!("rig: SAI interface failed to start");
            AUDIO_EXIT.store(1, Ordering::Relaxed);
            None
        }
    };

    // Linger on the pre-init red so it can actually be seen. Everything above this point takes a
    // few milliseconds, so without the delay red -> green reads as "always green" and the two
    // stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    asperitas_logging::led::set_global_state(if interface.is_some() {
        asperitas_logging::led::LedState::Running
    } else {
        asperitas_logging::led::LedState::Panicked
    });

    // Say what is about to play, and what the ring can hold, once, before anything plays it.
    emit_descriptors(generator, time_base);
    emit_capmax();

    // Priority first, then `start`. `InterruptExecutor::start()` documents that the priority must
    // be set before it and MUST NOT be touched after; setting it later is not merely late, it is
    // undefined, because the executor's pend-from-software assumes the vector's slot is final.
    //
    // P6 sits below the SAI DMA streams and the embassy-time driver (TIM5), which is the whole
    // argument that the callback can run without a critical section: the transfers it depends on
    // finish ahead of it rather than underneath it. The DMA side is P0 because `Config::default()`
    // ships `dma_interrupt_priority: Priority::P0` (embassy-stm32 0.6.0 `src/lib.rs:362`), and the
    // direction is required, not incidental: the DMA ISR is what pends SAI1, so it must outrank
    // the executor it wakes. The numbers are logged below as a reading of the NVIC, not as a
    // restatement of this comment.
    if let Some(interface) = interface {
        interrupt::SAI1.set_priority(Priority::P6);
        let audio_spawner = AUDIO_EXECUTOR.start(interrupt::SAI1);
        audio_spawner
            .spawn(audio_task(interface, generator, ring).expect("failed to spawn audio task"));
    }

    // Effective NVIC priorities, read back after the vector was unmasked. Lower numbers win on
    // Cortex-M (P0 is the highest, seven implemented bits, no subpriority on this part), so the
    // claim being checked is `tim5 < sai1` and `dma1_* < sai1`.
    info!(
        "rig: nvic priority sai1={} dma1_stream0={} dma1_stream1={} tim5={}",
        NVIC::get_priority(interrupt::SAI1),
        NVIC::get_priority(interrupt::DMA1_STREAM0),
        NVIC::get_priority(interrupt::DMA1_STREAM1),
        NVIC::get_priority(interrupt::TIM5),
    );

    // Thread mode is left to what must not run at audio priority: the console drain, which is also
    // the reporting task that paces `STATUS`, the boot LED, `CAPSTAT`, and the capture timeline with
    // its dump. None of them has a deadline.
    #[cfg(feature = "log-usb")]
    let console_fut = asperitas_logging::usb::run();
    // No drain task exists without the console, and RTT needs no task to exist at all - the
    // probe reads RAM behind our back. `pending()` stands in so this future is written once
    // instead of twice: it never wakes, exactly like the drain task it replaces.
    #[cfg(not(feature = "log-usb"))]
    let console_fut = core::future::pending::<()>();
    let led_fut = asperitas_logging::led::blink_task();
    // The timeline finishes after its dump; the reporter never does, so the join never completes.
    let rig_fut =
        embassy_futures::join::join(report_capstat(time_base), run_capture(ring, time_base));

    // All three loop forever, so reaching past the select means one of them returned: the thread
    // executor parks and the audio interrupt keeps playing, which is a machine still making sound
    // and reporting nothing about it. Halt rather than become that.
    embassy_futures::select::select3(console_fut, led_fut, rig_fut).await;
    error!("rig: console, LED or capture reporting returned; stopping");
    #[allow(clippy::empty_loop)]
    loop {}
}
