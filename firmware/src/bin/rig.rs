//! Measurement-rig binary: plays a known stimulus, captures one lane, and hands the bytes to
//! the host for objective measurement.
//!
//! This file deliberately stays close to `main.rs`. Everything it keeps - the preamble, the
//! board setup, the codec start, the block geometry, the encode step - behaves exactly as it
//! does in the binary already proven on hardware, which is the only reason to trust the parts
//! that are new. Three things differ, and they are the whole point:
//!
//! 1. `cortex_m::Peripherals::take()` is claimed, because rig owns DWT's cycle counter and
//!    measures durations with it (a duration measured against a counter nobody enabled reads
//!    zero and looks like a fast device).
//! 2. The audio future runs on the shared audio module's interrupt-mode executor woken by SAI1,
//!    not on the thread executor's select tree, so its latency has a bound rather than a hope.
//! 3. The DSP chain is a stimulus generator chosen by cargo feature, and the input frame never
//!    reaches it: rig measures the box, not the instrument plugged into it. A generator that plays
//!    once (the sweep) is held silent until the capture's first callback, so the recording holds it
//!    whole; a periodic one plays from the first callback, as before.
//!
//! What it then does with the input is the measurement. One lane of the codec's input is truncated
//! to `i16` and copied into a 32 MiB ring in SDRAM, the ring is shipped to the host over the framed
//! console once the window closes, and the timing of every callback is measured against DWT so the
//! host can tell a clean capture from a starved one. The run is a fixed timeline with no runtime
//! control: see "Capture timeline" below.
//!
//! The one thing rig does read from the host is an excerpt install: framed records on
//! the CDC OUT endpoint, written sector by sector into a QSPI slot and verified by readback. See
//! "Excerpt install" below; it never runs alongside a capture.

#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use asperitas_dsp::fx_chain::FxChain;
use asperitas_dsp::processor::{Frame, Processor};
#[cfg(feature = "stim-ess")]
use asperitas_dsp::stimulus::ExponentialSweep;
#[cfg(feature = "stim-pulse")]
use asperitas_dsp::stimulus::PulseTrain;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse", feature = "stim-excerpt")))]
use asperitas_dsp::stimulus::Sine;
use asperitas_dsp::stimulus::Stimulus;
use asperitas_firmware::audio::{self, measure_time_base, AudioProcessor, TimeBase, BLOCK_LENGTH};
use asperitas_logging::{error, info};
use asperitas_wire::capture::{self, BlockState};
use asperitas_wire::console;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use cortex_m::peripheral::{DWT, NVIC};
use daisy_embassy::audio::{AudioConfig, Fs};
use daisy_embassy::hal::interrupt;
use daisy_embassy::hal::interrupt::{InterruptExt, Priority};
use daisy_embassy::hal::{bind_interrupts, dma, peripherals, qspi, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;

use asperitas_firmware as _;

// rig's own USB IRQ struct. Each binary defines one: `bind_interrupts!` expands to an
// `#[export_name]` thunk per vector, so sharing the struct across crates would mean sharing its
// handlers with binaries that may not even enable the console.
//
// SAI1 is deliberately absent from this struct - see the handler written by hand below.
bind_interrupts!(pub struct RigUsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// The QSPI flash's two vectors, for `FlashBuilder::build_async` (the shape of daisy-embassy's
// `examples/flash.rs`). A struct of their own rather than two more lines in `RigUsbIrqs`, because the
// two have nothing in common but this binary: USB is bound whatever the transport, flash whether or
// not anything installs, and folding them together would make the next reader wonder which half the
// other depends on. SAI1 stays out of both, for the reason given at its hand-written handler.
bind_interrupts!(pub struct RigFlashIrqs {
    QUADSPI => qspi::InterruptHandler<peripherals::QUADSPI>;
    MDMA => dma::InterruptHandler<peripherals::MDMA_CH0>;
});

// ---------------------------------------------------------------------------
// Stimulus selection
// ---------------------------------------------------------------------------

/// How many of the four generators a build selected. Zero is a valid answer and means sine.
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
    if cfg!(feature = "stim-excerpt") {
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
    "select at most one stimulus: stim-sine, stim-ess, stim-pulse or stim-excerpt"
);

/// The generator this build plays.
///
/// The sine arm's `cfg` covers both spellings of "sine": explicitly `--features stim-sine`, and
/// no stimulus feature at all. Mutual exclusion above is what makes leaving `stim-sine` out of
/// that condition correct rather than a subtle wrong-build generator.
#[cfg(feature = "stim-ess")]
type BareGenerator = ExponentialSweep;
#[cfg(feature = "stim-pulse")]
type BareGenerator = PulseTrain;
#[cfg(feature = "stim-excerpt")]
type BareGenerator = ExcerptPlayer;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse", feature = "stim-excerpt")))]
type BareGenerator = Sine;

/// What the audio task holds: the generator, then the replay effect (`CTL FX`), on every build.
/// With no `FX` sent the effect is bypass and the output is bit-identical to the bare generator
/// (`asperitas_dsp::fx_chain`).
type ActiveGenerator = FxChain<BareGenerator>;

/// The chain around `gen`: shipped smoothing, or none under `unsmoothed-params`.
fn chain_of(gen: BareGenerator) -> ActiveGenerator {
    #[cfg(feature = "unsmoothed-params")]
    {
        FxChain::new_unsmoothed(gen)
    }
    #[cfg(not(feature = "unsmoothed-params"))]
    {
        FxChain::new(gen)
    }
}

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

// The SAI1 vector that drives the shared audio executor.
asperitas_firmware::audio_sai1_vector!();

/// Emit one wire-contract body to the framed console, stamped with the clock as it is now.
///
/// Two definitions rather than a `cfg` at each call site, because there are many (RIGCFG, RIGGEN,
/// CAPMAX, CAPSTAT, STIMSTART, DUMPEND) and that many copies of `#[cfg(feature = "log-usb")]` is
/// that many ways to forget one.
///
/// Under `log-defmt` without `log-usb` this discards, and that is honest rather than a gap: the
/// verbs describe the byte stream the framed console carries, and an RTT-only image captures no
/// such stream. The shim exists so that build still links (CI's second configuration) without
/// pretending a measurement record went somewhere.
#[cfg(feature = "log-usb")]
fn emit_console(body: &[u8]) {
    if !asperitas_logging::emit_record(asperitas_logging::Level::Info, now_ms(), body) {
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
// running board (docs/reference/daisy-seed3.md, "External SDRAM"). What would change
// this hand-off is an MPU region over 0xC000_0000: whoever adds one owns the coherence argument for
// the FMC window and revisits these orderings in the same change. Device memory also takes no
// unaligned accesses, so the ring is only ever touched through aligned copies.

/// Which input lane the ring keeps.
///
/// Left, matching `main.rs`'s `decode_block`, which reads `words[0]` as left from the same
/// interleaved layout. Which physical jack that is on the Pod is a bench observation to make
/// at the bench; if it is the wrong one, flipping this constant is the whole fix, because
/// [`MONO_WORD`] and `RIGCFG`'s `lane` both derive from it.
const MONO_LANE: console::MonoLane = console::MonoLane::Left;

/// The interleaved word offset [`MONO_LANE`] names.
const MONO_WORD: usize = match MONO_LANE {
    console::MonoLane::Left => 0,
    console::MonoLane::Right => 1,
};

/// What closes a capture window.
#[derive(Clone, Copy)]
enum Window {
    /// A deadline this many seconds after arming.
    Seconds(usize),
    /// No deadline of its own: the window runs until the producer finds no `Free` block and
    /// disarms itself, so it holds exactly the whole ring. This is the run that checks `CAPMAX`'s
    /// `seconds_max` against the wall clock.
    RingFill,
}

/// The window this build records, from `ASP_RIG_CAPTURE_SECONDS` when set.
///
/// The override exists so a bench run can change its window without editing source
/// (`ASP_RIG_CAPTURE_SECONDS=30 cargo build ...`, or `=ring` to fill the ring); rustc tracks the
/// variable, so changing it rebuilds. A number of seconds is held to the ring gate below, so it can
/// shorten the window freely and lengthen it only as far as the ring allows.
const WINDOW: Window = match option_env!("ASP_RIG_CAPTURE_SECONDS") {
    Some(text) => parse_window(text),
    None => Window::Seconds(capture::CAPTURE_WINDOW_SECONDS),
};

/// Parse the window at compile time: a whole number of seconds, or the word `ring`. Anything else
/// fails the build, naming the variable, instead of falling back to a default nobody asked for.
const fn parse_window(text: &str) -> Window {
    let bytes = text.as_bytes();
    assert!(
        !bytes.is_empty(),
        "ASP_RIG_CAPTURE_SECONDS is set but empty"
    );
    if let [b'r', b'i', b'n', b'g'] = bytes {
        return Window::RingFill;
    }
    let mut seconds = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i].is_ascii_digit(),
            "ASP_RIG_CAPTURE_SECONDS must be a whole number of seconds, or `ring`"
        );
        seconds = seconds * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    Window::Seconds(seconds)
}

// The parser's arms, checked where they are compiled. A value that fails to parse cannot be tested
// this way - it fails the build, which is the behaviour - so these pin the arms that must succeed.
const _: () = assert!(matches!(parse_window("ring"), Window::RingFill));
const _: () = assert!(matches!(parse_window("300"), Window::Seconds(300)));
const _: () = assert!(matches!(parse_window("0"), Window::Seconds(0)));

/// Whether this build fills the ring rather than closing on a deadline.
const RING_FILL: bool = matches!(WINDOW, Window::RingFill);

/// Seconds of capture this build records: the window's own figure, or for a ring fill the ring's
/// floored capacity (`CAPMAX`'s `seconds_max`, 349), which is what `RIGCFG`'s `window_s` reports.
const CAPTURE_SECONDS: usize = match WINDOW {
    Window::Seconds(seconds) => seconds,
    Window::RingFill => capture::ring_seconds_floor() as usize,
};

/// Ring blocks the window consumes: 879 at the default 300 s. Rounded up by `expected_blocks`,
/// because the producer always finishes the block it is in when the window closes. A ring fill
/// consumes every block.
const WINDOW_BLOCKS: usize = match WINDOW {
    Window::Seconds(seconds) => capture::expected_blocks(seconds),
    Window::RingFill => capture::RING_BLOCKS,
};

/// Bytes the window needs, as `CAPMAX`'s `total_bytes` reports them.
const WINDOW_BYTES: usize = match WINDOW {
    Window::Seconds(seconds) => seconds * capture::BYTES_PER_SECOND,
    Window::RingFill => capture::RING_BYTES,
};

/// Backstop past a ring fill's expected end, in seconds. The producer's disarm is what closes a
/// ring fill; this deadline only fires if it never comes (audio stopped mid-window), so the run still
/// judges and dumps what it holds instead of waiting forever, and the judge reports the shortfall.
const RING_FILL_BACKSTOP_SECONDS: usize = 10;

/// Seconds after arming at which the timeline closes the window if nothing closed it sooner.
const DEADLINE_SECONDS: usize = if RING_FILL {
    // Floored capacity plus the backstop: past the 349.5 s the ring actually takes to fill.
    CAPTURE_SECONDS + RING_FILL_BACKSTOP_SECONDS
} else {
    CAPTURE_SECONDS
};

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
// A window of seconds must fit the ring, because its deadline is what ends it. A ring fill is ended
// by the ring itself, so it is exempt, and its backstop must not close it before the ring is full.
const _: () = assert!(
    RING_FILL || WINDOW_BLOCKS < capture::RING_BLOCKS,
    "the capture window does not fit the ring; shorten ASP_RIG_CAPTURE_SECONDS"
);
const _: () = assert!(
    !RING_FILL || DEADLINE_SECONDS as u64 * 1_000_000 > capture::ring_duration_micros(),
    "a ring fill's backstop deadline would close the window before the ring is full"
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
/// Set by the capture timeline immediately before it arms and cleared only after its dump has
/// ended, so it covers armed, filling, the drain of the last block, judging and dumping as one span
/// with no gap between them. The excerpt installer refuses to touch flash while it is set. `ARMED`
/// alone would not do: the producer clears it on overrun, minutes before the dump that follows.
static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Set by the first audio callback.
static AUDIO_STARTED: AtomicBool = AtomicBool::new(false);
/// Longest callback and longest gap between callback starts, in raw DWT cycles. Converted to
/// microseconds only when reported, with the time base boot settled on.
static MAX_BLOCK_CYCLES: AtomicU32 = AtomicU32::new(0);
static WORST_GAP_CYCLES: AtomicU32 = AtomicU32::new(0);
/// Gaps whose raw cycle delta was too wide to trust (see [`Producer::on_callback`]).
static INVALID_GAPS: AtomicU32 = AtomicU32::new(0);
/// Ring-fill builds only: `embassy_time` milliseconds at the first armed callback and at the
/// producer's disarm, which bracket the device's own measure of how long the ring took to fill.
/// Zero means "not yet", which no real stamp can be: arming waits [`ARM_DELAY_MS`] past the first
/// callback. Milliseconds in a `u32` wrap after 49 days, and the single shot is over within
/// minutes of boot. DWT cannot time this: CYCCNT wraps every 8.9 s and the fill takes 349.5 s.
static FILL_STARTED_MS: AtomicU32 = AtomicU32::new(0);
static FILL_ENDED_MS: AtomicU32 = AtomicU32::new(0);
/// One-shot builds only: the block and callback whose input was being captured when the callback
/// rendered the stimulus's first sample. Written once, by that callback, before it sets
/// `STIM_STARTED` with release, so a reader that sees the flag sees both positions.
static STIM_STARTED: AtomicBool = AtomicBool::new(false);
static STIM_BLOCK: AtomicU32 = AtomicU32::new(0);
static STIM_CALLBACK: AtomicU32 = AtomicU32::new(0);

/// Where one callback's input landed in the ring: block sequence number and callback within it.
#[derive(Clone, Copy)]
struct Captured {
    block: u32,
    callback: usize,
}

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
    /// Record one callback: its entry time, then, if a capture is running, its input lane. Returns
    /// where that input landed, or `None` when nothing was captured.
    ///
    /// Called first thing in the callback with the cycle count taken on entry; the matching exit
    /// time goes to [`Producer::finish`].
    fn on_callback(&mut self, entry: u32, input: &[u32]) -> Option<Captured> {
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
    fn finish(&self, entry: u32) {
        let ran = DWT::cycle_count().wrapping_sub(entry);
        MAX_BLOCK_CYCLES.fetch_max(ran, Ordering::Relaxed);
    }
}

/// The `embassy_time` clock in whole milliseconds, truncated to the width the fill stamps keep.
fn now_ms() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}

/// Raw cycles to whole microseconds, with the time base boot settled on.
fn cycles_to_us(cycles: u32, time_base: TimeBase) -> u32 {
    cycles / time_base.cycles_per_us
}

/// The one `CAPMAX` record: what this build's ring could hold, all from `capture::` arithmetic.
fn emit_capmax() {
    let cap = console::RingCapacity {
        total_bytes: WINDOW_BYTES as u32,
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
        audio_exit: audio::AUDIO_EXIT.load(Ordering::Relaxed),
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
    if RING_FILL {
        // The one overrun is the full ring refusing the next claim: the window's terminator, not a
        // loss. Zero means the backstop deadline closed the window instead, and the ring never filled.
        info!(
            "rig: gate overrun {} == 1 (the full ring ending the window, expected): {}",
            st.overrun,
            verdict(st.overrun == 1)
        );
        let started = FILL_STARTED_MS.load(Ordering::Relaxed);
        let ended = FILL_ENDED_MS.load(Ordering::Relaxed);
        if started != 0 && ended != 0 {
            info!(
                "rig: ring filled in {} ms, first armed callback to disarm (CAPMAX seconds_max {}, us_max {})",
                ended.wrapping_sub(started),
                capture::ring_seconds_floor(),
                capture::ring_duration_micros(),
            );
        } else {
            error!(
                "rig: ring fill not timed (started_ms={started}, ended_ms={ended}); the ring never filled: FAIL"
            );
        }
    } else {
        info!(
            "rig: gate overrun {} == 0: {}",
            st.overrun,
            verdict(st.overrun == 0)
        );
    }
}

/// One-shot builds: check the stimulus played whole inside the window just closed, and emit
/// `STIMSTART` saying where it began. `samples` is the stimulus's length from `one_shot_samples`.
///
/// Says nothing rather than something wrong: no `STIMSTART` when the stimulus never started (the
/// window captured nothing) or started somewhere the window does not contain, which the start rule
/// in the audio callback makes unreachable and this reports rather than trusts.
fn report_stimulus_start(first_block: u32, end_block: u32, samples: u32) {
    if !STIM_STARTED.load(Ordering::Acquire) {
        error!("rig: the one-shot stimulus never started; no STIMSTART");
        return;
    }
    let block = STIM_BLOCK.load(Ordering::Relaxed);
    let callback = STIM_CALLBACK.load(Ordering::Relaxed) as usize;
    let Some(offset) = capture::window_sample_offset(first_block, block, callback) else {
        error!(
            "rig: stimulus started at block {block} callback {callback}, outside the window from block {first_block}; no STIMSTART"
        );
        return;
    };
    let captured = u64::from(end_block - first_block) * capture::samples_per_block() as u64;
    let end = u64::from(offset) + u64::from(samples);
    let verdict = if end <= captured { "pass" } else { "FAIL" };
    info!(
        "rig: gate stimulus samples {offset}..{end} inside the {captured}-sample capture: {verdict}"
    );
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::stimstart_body(
        &console::StimulusStart {
            first_block,
            offset,
        },
        &mut body,
    );
    emit_console(&body[..n]);
}

// ---------------------------------------------------------------------------
// Capture timeline
// ---------------------------------------------------------------------------
//
// rig arms itself. There is no inbound control channel to arm it through (runtime control is
// the control channel; the inbound path rig does read carries excerpt installs and nothing else), so the run is a
// fixed timeline, repeated by resetting the board, which `slow-boot` keeps safe for DFU:
//
//   boot      BOOT, RIGCFG, RIGGEN, CAPMAX, then audio starts; a periodic stimulus plays from here,
//             a one-shot one (the sweep) outputs silence until the first captured callback
//   +2 s      after the first callback: arm (ARM_DELAY_MS); the first captured callback starts a
//             one-shot stimulus, so the window holds it from its first sample
//   +window   disarm; the producer finishes the block it is in, so exactly WINDOW_BLOCKS land
//             - or earlier, if the ring fills and the producer disarms itself on overrun
//             A ring-fill build (`ASP_RIG_CAPTURE_SECONDS=ring`) has no window of seconds: that
//             overrun is how it ends, after exactly RING_BLOCKS, ~349.5 s. Its deadline is only a
//             backstop (RING_FILL_BACKSTOP_SECONDS past the ring's capacity) for audio that stops.
//   then      judge the window; a one-shot build checks the stimulus fit and emits STIMSTART; dump
//             every captured block in ring order, emit DUMPEND
//   after     idle; CAPSTAT keeps reporting once a second
//
// The dump waits for the window to close rather than draining alongside it so the measured window
// carries no bulk USB traffic: worst_gap_us and max_block_us then describe the audio path alone. The
// gates guarantee the window fits the ring, and a ring fill stops rather than overwrite, so nothing is
// lost by waiting.
//
// Single-shot looks like a missing feature. It is the absence of an inbound control channel, stated
// here so nobody adds a re-arm path that nothing can trigger.
//
// `CAPTURE_ACTIVE` spans arm to `DUMPEND`. An excerpt install is refused for that whole span, and one
// already open when it begins is abandoned at its next record: see "Excerpt install".

/// Run the timeline above once, then idle. `one_shot` is the stimulus's `one_shot_samples`.
async fn run_capture(ring: Ring, time_base: TimeBase, one_shot: Option<u32>) {
    while !AUDIO_STARTED.load(Ordering::Relaxed) {
        if audio::AUDIO_EXIT.load(Ordering::Relaxed) != 0 {
            error!("rig: audio never started; nothing to capture");
            return;
        }
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }
    embassy_time::Timer::after_millis(ARM_DELAY_MS).await;

    let first_block = PRODUCED.load(Ordering::Acquire);
    if RING_FILL {
        info!("rig: capture armed until the ring is full ({WINDOW_BLOCKS} blocks)");
    } else {
        info!("rig: capture armed for {CAPTURE_SECONDS} s ({WINDOW_BLOCKS} blocks)");
    }
    CAPTURE_ACTIVE.store(true, Ordering::Release);
    ARMED.store(true, Ordering::Release);

    let deadline =
        embassy_time::Instant::now() + embassy_time::Duration::from_secs(DEADLINE_SECONDS as u64);
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
    if let Some(samples) = one_shot {
        report_stimulus_start(first_block, end_block, samples);
    }
    dump_ring(ring, first_block, end_block).await;
    CAPTURE_ACTIVE.store(false, Ordering::Release);
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
    use asperitas_wire::{dump, frame};

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

// ---------------------------------------------------------------------------
// Excerpt install
// ---------------------------------------------------------------------------
//
// The host streams `EXCSTART` / `EXCDATA` / `EXCEND` records (`asperitas_wire::excerpt`) over the
// CDC OUT endpoint; `usb::inbound()` hands them over whole and CRC-checked; `excerpt::Installer` turns
// them into flash actions; the task below performs each action and nothing else. Every protocol
// decision - ordering, short and overrun chunks, the stream CRC, when a sector is full, whether the
// readback earns an `EXCOK` - is the installer's, and is covered by its host suite
// (`crates/asperitas-host/tests/excerpt_install.rs`, including `dropping_any_frame_never_verifies`
// for truncation and the CRC-mismatch verdict tests for corruption). This side adds only flash I/O,
// the capture gate and the reporting, so a truncated or corrupted upload reaches `EXCOK` here exactly
// when it does there: never.
//
// ### Only the async flash API, and what that costs
//
// daisy-embassy's blocking `read` / `write` / `erase` wait on the chip's WIP bit in an unbounded busy
// loop (`wait_for_write`): a stuck chip would hang thread mode, and with it the console that could
// say so. The async calls auto-poll the status register in QUADSPI hardware and wake on its match
// interrupt, so an erase pause is an await that costs thread mode nothing. Their price is that each
// wait is bounded by `with_timeout(..).expect(..)` - 600 ms per sector erase, 1.6 ms per 256-byte page
// program, twice the datasheet maxima - so a chip that misses one panics, and the panic handler turns
// that into a `PANIC` record and a red LED. That is the chosen failure: loud and attributable, rather
// than a silent hang. `firmware/build.rs` fails the build if this file ever calls a blocking flash
// method or `FlashBuilder::build`. The one blocking wait left in the image is the driver's own
// `reset()` inside `build_async`, run once at boot before audio starts.
//
// Wall clock is dominated by erases: a full 516,096-byte slot is 127 PCM sectors plus the header
// sector, and at the IS25LP064A's typical sector erase (tens of milliseconds) that is seconds; a
// 288,000-byte corpus clip is 71 + 1. Predicted against observed is the bench table's job, not a
// figure this comment can know.
//
// ### Why an install cannot disturb the capture
//
// 1. It never overlaps one. `CAPTURE_ACTIVE` spans arm to `DUMPEND`; an `EXCSTART` inside it is
//    refused with `EXCFAIL ... why=busy`, and an install already open when it begins is abandoned the
//    same way at its next record, so at most the one flash operation in flight runs into the window.
// 2. It runs on the thread executor only. The SAI1 callback never names `FLASH`, the installer or
//    anything below; its world is the ring and the atomics above.
// 3. Its interrupts sit below the audio executor. `hal::init` gives MDMA `Priority::P0` (embassy-stm32
//    0.6.0 `Config::default()`) and `Qspi::new_bank1` enables QUADSPI at the reset value, also P0, so
//    both would preempt the P6 callback. `main` drops both to P7 right after `build_async`. Neither
//    handler wakes anything at audio priority - each wakes a thread-mode task - so there is no reason
//    for them to outrank SAI1, unlike the SAI DMA streams, which pend it. At P7 they preempt the
//    callback zero times, and the most the callback can do to them is delay a wake by one callback
//    (~76 us against the 1.6 ms page timeout). Priced at P0 instead, for whoever moves them: one page
//    program is one MDMA transfer-complete plus one QUADSPI status-match interrupt, a few hundred
//    cycles each, and at the datasheet's typical ~0.2 ms per page at most four pages fit a 666 us
//    period - about eight ISRs, a few microseconds, against the ~590 us of headroom the callback
//    leaves. Affordable, but not free, and not needed. The `nvic priority` line at boot reads both
//    back.
//
// Bus traffic is the remaining coupling: MDMA writes the readback into AXI SRAM (the staging's own region, either layout), which the SAI DMA
// also reaches. Arbitration there is per beat, nanoseconds, and SDRAM (the ring) is not on the path.

/// The board's QSPI flash, async-only (see above).
type RigFlash = daisy_embassy::flash::Flash<'static, hal::mode::Async>;

/// The one flash handle, built once in `main` before audio starts.
///
/// A mutex because two thread-mode tasks will want it: the excerpt installer here, and the replay
/// refill. `ThreadModeRawMutex` because thread mode is the only place flash is
/// ever touched - the raw mutex panics if an interrupt tries - and the `Option` because a `static`
/// cannot run `build_async`. The rule for every user: hold the guard across your own flash awaits
/// only, never across a channel receive or a timer, or the other user stalls behind a host that is
/// thinking.
static FLASH: Mutex<ThreadModeRawMutex, Option<RigFlash>> = Mutex::new(None);

/// Sector size as a `usize`, for array lengths. The installer lends sectors as `[u8; SECTOR]`.
#[cfg(feature = "log-usb")]
const SECTOR_BYTES: usize = asperitas_wire::excerpt::SECTOR as usize;

/// Erase or rewrite one whole sector of `slot`: erase when `data` is `None`, erase then program
/// `data` otherwise. The only place this binary erases or writes QSPI.
///
/// The asserts run before the driver is called, because the driver erases the whole 4 KiB sector
/// containing any address it is handed and asserts nothing but the chip's top address: an
/// unaligned or out-of-slot address would silently take a neighbouring slot's sector with it.
/// Length needs no runtime check - `[u8; SECTOR_BYTES]` is the only shape `data` can have - so
/// every write is exactly one sector at a sector boundary inside the slot it names.
#[cfg(feature = "log-usb")]
async fn rewrite_sector(
    flash: &mut RigFlash,
    slot: u8,
    address: u32,
    data: Option<&[u8; SECTOR_BYTES]>,
) {
    use asperitas_wire::excerpt::{slot_base, SECTOR, SLOT_SPAN};
    let base = slot_base(slot);
    assert!(
        base.is_some_and(|base| address.is_multiple_of(SECTOR)
            && address >= base
            && address + SECTOR <= base + SLOT_SPAN),
        "rig: flash sector write outside its slot"
    );
    match data {
        Some(data) => flash.write_async(address, data).await,
        None => flash.erase_async(address, SECTOR).await,
    }
}

/// Read `bytes` from `address` in sector-sized chunks and return their CRC-16 and how many bytes
/// were read. `scratch` is the chunk buffer; the range must lie inside `slot`.
#[cfg(feature = "log-usb")]
async fn readback_crc(
    flash: &mut RigFlash,
    slot: u8,
    address: u32,
    bytes: u32,
    scratch: &mut [u8; SECTOR_BYTES],
) -> (u16, u32) {
    use asperitas_wire::excerpt::{slot_base, SLOT_SPAN};
    use asperitas_wire::frame::{crc16_ccitt_update, CRC16_INITIAL};
    assert!(
        slot_base(slot).is_some_and(|base| address >= base && address + bytes <= base + SLOT_SPAN),
        "rig: flash readback outside its slot"
    );
    let mut crc = CRC16_INITIAL;
    let mut read = 0u32;
    while read < bytes {
        let n = (bytes - read).min(SECTOR_BYTES as u32) as usize;
        flash.read_async(address + read, &mut scratch[..n]).await;
        crc = crc16_ccitt_update(crc, &scratch[..n]);
        read += n as u32;
    }
    (crc, read)
}

/// The flash handle behind a held `FLASH` guard.
///
/// `main` stores the handle before it starts polling anything that calls this, so `None` here is a
/// boot-order bug, not a runtime condition worth a quieter answer.
#[cfg(any(feature = "log-usb", feature = "stim-excerpt"))]
fn flash_of<'g>(
    guard: &'g mut embassy_sync::mutex::MutexGuard<'_, ThreadModeRawMutex, Option<RigFlash>>,
) -> &'g mut RigFlash {
    guard
        .as_mut()
        .expect("rig: FLASH is stored before the install task runs")
}

/// Report a failed install as `EXCFAIL`.
#[cfg(feature = "log-usb")]
fn emit_exc_fail(failure: &asperitas_wire::excerpt::Failure) {
    let mut body = [0u8; asperitas_wire::frame::MAX_BODY];
    let n = asperitas_wire::excerpt::exc_fail_body(failure, &mut body);
    emit_console(&body[..n]);
}

/// Answer one `CTL` record. The rig serves the replay-effect verbs `FX`, `SET` and `REL` on every
/// build (see "Replay effect" below); every other verb gets `unknown-verb`, as before this channel existed
/// it was ignored, and a host that waits for an answer is better told. Parse failures are counted
/// and answered by `route`.
#[cfg(feature = "log-usb")]
fn handle_control(record: &asperitas_logging::inbound::InboundRecord) {
    use asperitas_wire::control::{route, ErrorCode, Reply, CONTROL};
    let mut reply = None;
    let rejected = route(
        record,
        &CONTROL,
        |_| {},
        |cmd| {
            reply = Some(match cmd {
                asperitas_wire::control::Command::ReplayFx { fx, value } => {
                    // Changing the effect mid-capture would make the recording unrepeatable.
                    if CAPTURE_ACTIVE.load(Ordering::Acquire) {
                        Reply::Error(ErrorCode::Busy)
                    } else if fx_ctl::select_fx(fx, value) {
                        Reply::Ok(cmd.verb())
                    } else {
                        Reply::Error(ErrorCode::BadValue)
                    }
                }
                // Allowed mid-capture, unlike `FX`: the effect kind and base value stay fixed
                // while the override moves the parameter, which is what a sweep is.
                asperitas_wire::control::Command::OverrideSet { knob, value } => {
                    if fx_ctl::override_set(knob, value) {
                        Reply::Ok(cmd.verb())
                    } else {
                        Reply::Error(ErrorCode::BadKnob)
                    }
                }
                asperitas_wire::control::Command::OverrideRelease { knob } => {
                    if fx_ctl::override_release(knob) {
                        Reply::Ok(cmd.verb())
                    } else {
                        Reply::Error(ErrorCode::BadKnob)
                    }
                }
                _ => Reply::Error(ErrorCode::UnknownVerb),
            });
        },
    );
    if let Some(r) = reply.or(rejected) {
        let mut body = [0u8; asperitas_wire::frame::MAX_BODY];
        let n = r.encode(&mut body);
        emit_console(&body[..n]);
    }
}

/// Consume install records forever: the one consumer of `usb::inbound()`.
///
/// Emits exactly one verdict per install - `EXCOK` after the header is written, or one `EXCFAIL` -
/// and no progress records: the outcome is what the host waits for, and per-sector chatter would
/// share the pipe with everything else for no reader. Holds `FLASH` for one action at a time.
///
/// The installer's 4 KiB sector buffer and the 4 KiB `scratch` live in this future, which is part of
/// `main`'s statically allocated task, not on the stack.
#[cfg(feature = "log-usb")]
async fn run_install(inbound: asperitas_logging::usb::InboundReceiver) {
    use asperitas_wire::excerpt::{
        exc_ok_body, parse_record, Action, ExcRecord, Installer, RecordError, Verdict, Why,
        HEADER_LEN,
    };

    let mut installer = Installer::new();
    let mut scratch = [0u8; SECTOR_BYTES];
    // The slot the open install named in its `EXCSTART`, so every sector write is bounds-checked
    // against the slot the host asked for, not against one re-derived from the address being checked.
    let mut target: Option<u8> = None;
    loop {
        let record = inbound.receive().await;
        let body = record.body();
        if body.starts_with(asperitas_wire::control::CMD_PREFIX) {
            handle_control(&record);
            continue;
        }
        let parsed = parse_record(body);
        if matches!(parsed, Err(RecordError::NotExcerpt)) {
            continue;
        }

        // The capture gate, checked per record. Checking and acting happen with no await between
        // them, and the timeline sets `CAPTURE_ACTIVE` on this same thread executor, so the two
        // cannot interleave inside that span.
        let starting = matches!(parsed, Ok(ExcRecord::Start { .. }));
        // A replay pass counts too: from boot until the callback has handed over the last sample, the
        // refill task is reading the very slot an install would be erasing.
        let busy = CAPTURE_ACTIVE.load(Ordering::Acquire) || replay_pass_open();
        if busy && (starting || installer.is_open()) {
            info!("rig: excerpt install refused: a capture is armed or dumping, or a replay pass is open");
            emit_exc_fail(&installer.abort(Why::Busy));
            continue;
        }

        let action = match parsed {
            Ok(parsed) => installer.feed(parsed),
            // Re-parsed inside, which is what turns the grammar error into `why=malformed`.
            Err(_) => installer.feed_body(body),
        };
        match action {
            Action::InvalidateHeader {
                slot,
                address,
                aborted,
            } => {
                if let Some(old) = aborted {
                    info!("rig: excerpt install for slot {old} abandoned by a new EXCSTART");
                }
                info!("rig: excerpt install into slot {slot} started");
                target = Some(slot);
                let mut guard = FLASH.lock().await;
                rewrite_sector(flash_of(&mut guard), slot, address, None).await;
            }
            Action::WriteSector { address, data, .. } => {
                let slot = target.expect("rig: the installer wrote a sector with no EXCSTART seen");
                let mut guard = FLASH.lock().await;
                rewrite_sector(flash_of(&mut guard), slot, address, Some(data)).await;
            }
            Action::Verify {
                slot,
                address,
                bytes,
            } => {
                let mut guard = FLASH.lock().await;
                let flash = flash_of(&mut guard);
                let (crc, len) = readback_crc(flash, slot, address, bytes, &mut scratch).await;
                match installer.verdict(crc, len) {
                    Verdict::Ok {
                        slot,
                        address,
                        header,
                        encoded,
                    } => {
                        // The header goes last and as a whole sector: its presence is the proof
                        // that everything before it read back clean.
                        scratch.fill(0xFF);
                        scratch[..HEADER_LEN].copy_from_slice(&encoded);
                        rewrite_sector(flash, slot, address, Some(&scratch)).await;
                        drop(guard);
                        info!("rig: excerpt slot {slot} installed and verified");
                        let mut out = [0u8; asperitas_wire::frame::MAX_BODY];
                        let n = exc_ok_body(&header, &mut out);
                        emit_console(&out[..n]);
                    }
                    Verdict::Fail(failure) => {
                        drop(guard);
                        error!("rig: excerpt readback did not match; slot {slot} left blank");
                        emit_exc_fail(&failure);
                    }
                }
            }
            Action::Reject(failure) => emit_exc_fail(&failure),
            Action::Accepted | Action::Ignored => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Excerpt replay
// ---------------------------------------------------------------------------
//
// `stim-excerpt` plays a slot installed by the section above. The audio callback never touches QSPI:
// it pops 32-sample blocks from `asperitas_replay::ReplayCore`, a two-half ping-pong in
// internal RAM whose accounting (which side may touch which half, the underrun policy, the CRC) is
// host-tested in that crate. Everything here is the part a host cannot test: the flash read that fills
// a half, the cache argument, and the reporting.
//
// ### Boot order
//
// `load` runs in `main` between the flash build and `prepare_interface`: it reads and validates the
// slot header, starts the pass, and fills BOTH halves before any audio exists. The callback therefore
// finds a `Ready` half on its first block and the SAI start timing is unchanged. An empty or invalid slot
// does not play silence: `halt_unplayable` sends `EXCPLAYFAIL`, turns the LED red and serves the console
// forever with no audio.
//
// ### When the pass runs
//
// `ExcerptPlayer::one_shot_samples` is `Some`, so the audio task holds the player silent until the
// capture's first callback and then records where it started (`STIMSTART`), exactly as for the sweep:
// the recording holds the excerpt whole. `reset()` is therefore a no-op - the pass was started and
// staged at boot, and restarting it from the callback would mean flash work at audio priority.
//
// ### Where the staging lives, and the D-cache
//
// The staging is `REPLAY`, placed by name in the `.replay_bss` section (NOLOAD, zeroed at boot by
// `zero_staging`) and always in AXI SRAM: the whole RAM region under `memory-internal.x`, and the 32 KB
// `AXI_TAIL` above the 480 KB bootloaded image under `memory-bootloaded.x`. AXI SRAM is cacheable by the
// default memory map. It is deliberately not the DTCM that plain `.bss` is under the bootloaded layout
//: MDMA, the master `Flash::read_async` uses, reaches the TCMs only through its separate AHBS
// port, DMA1/DMA2 cannot reach DTCM at all, and nothing here confirmed from RM0433 that an MDMA refill into
// DTCM lands. `scripts/firmware-variants.sh` (the `replay` action) checks the symbol's address in the bootloaded image.
// The argument for either cache state, which `fill_half` carries out:
//
// * D-cache off (nothing in this stack enables it today; RIGCFG reports the live bit): MDMA's writes
//   are the only writes, loads go to memory, no maintenance is needed and none is done.
// * D-cache on (the buffer is in AXI SRAM): MDMA writes memory behind the cache. The CPU never writes a
//   staging byte (the callback only reads; `take_block` takes `&self` and the halves are written only
//   through a `FillClaim`), so no line over a half is ever dirty and nothing can be written back over
//   the DMA's data. The only hazard is a stale clean line from the half's previous contents, so after
//   each read completes `fill_half` invalidates the half's cache lines before publishing it. The
//   range is rounded up to a whole line; halves are 32-byte aligned and a multiple of 32 long
//   (compile-time asserts in `replay`), so the rounding stays inside the half.
//
// ### The interrupt budget the callback keeps
//
// Added to the SAI1 callback by this path: one `take_block` per callback. Worst case it is a CRC-16
// over 64 bytes (`crc16_ccitt_update` is the bitwise form: 8 shift-and-conditional-xor steps per byte,
// priced pessimistically at 6 cycles each, so 64 x 8 x 6 = 3 072 cycles), 32 sample unpacks and 32
// float conversions (about 10 cycles each, 640), a few atomic loads and stores (under 100), and the
// block-boundary half hand-off (about 50): roughly 3 900 cycles, 8.2 us at 480 MHz. That is about 1.2%
// of the 666 us period against the ~590 us of headroom the sine generator leaves today, and it replaces
// the generator's own per-sample work rather than adding to it, so `max_block_us` is not worsened by
// more than the estimate and is expected to fall. An estimate, not a reading: the observed figure is
// `CAPSTAT`'s `max_block_us` on hardware. No bench claim is made here.
//
// ### Replay effect
//
// `CTL FX f=<2 hex> v=<4 hex>` selects the effect stage that follows the generator: f 00 bypass (the
// default), 01 the `asperitas_dsp::Asperitas` patch, the same filter-then-gain chain `main` runs, at
// its default knob positions (filter open, 0 dB). v is accepted for wire stability and ignored.
// Bypass touches no sample, so default captures are bit-exact with the bare generator. The
// effect runs after `take_block`, so EXCPLAY's CRC still describes the raw stored samples.
// The pass is held until the capture's first callback, which is `ARM_DELAY_MS` after audio starts,
// so the control must arrive in that window; it is refused with `busy` once a capture is armed.
// `select_fx` publishes under a critical section and the callback picks it up at the next block.
//
// `CTL SET k=<00|01> v=<4 hex>` / `CTL REL k=<00|01>` override knob 0 (filter cutoff) or knob 1
// (output gain) through the same `asperitas_pod::knob_override::KnobOverride` the shipped firmware
// uses. Unlike `FX` it is accepted while a capture is armed, so `asperitas-rig`'s override sweep
// moves the effect mid-capture. Each callback merges the overrides over the patch's default knob
// positions (the "pot reading") and calls `ReplayFx::set_knobs`, which acts only when the merged
// pair changed; the override therefore enters UPSTREAM of the effect's smoothing, exactly where
// `main` applies it. Releasing returns a knob to its default. Bypass ignores both. Knob ids other
// than 0 and 1 get `bad-knob`.
//
// Feature `unsmoothed-params` builds the effect without smoothing (`ReplayFx::new_unsmoothed`): the
// parameters step on the next sample. Deliberately click-prone, for proving the discontinuity
// detector sees steps; the default build keeps the shipped smoothing.
//
// Callback cost: when nothing changes, one critical section around a two-element copy plus a
// compare, a few tens of cycles. While a value moves, one `set_controls` per block re-derives the
// parameters (a `powf` for gain, a `powf` and `expf` for the filter).
//
// ### What EXCPLAY's CRC says
//
// It is the CRC of the samples handed to the DAC encoder (`asperitas_replay::PassReport::crc16`). With `want`
// (the stored header CRC) and `clean=1` it says QSPI-to-encoder delivered the excerpt intact and
// unbroken. It says nothing about the codec, the analog path or the cable.
// ---------------------------------------------------------------------------
// Replay effect control (every build)
// ---------------------------------------------------------------------------

/// The host's handle on the effect stage that follows the generator. Compiled on every build, not
/// only `stim-excerpt`: the sine, sweep and pulse builds run the same stage.
mod fx_ctl {
    use asperitas_dsp::fx_chain::FxChain;
    use asperitas_dsp::processor::{Frame, Processor};
    use asperitas_dsp::KNOB_COUNT;
    use asperitas_pod::knob_override::KnobOverride;
    use core::cell::{Cell, RefCell};
    use core::sync::atomic::{AtomicU32, Ordering};
    use cortex_m::interrupt::Mutex;

    /// The pending effect selection `(kind code, value)` and a counter the callback compares with the
    /// one it last applied, so the common case is a single relaxed atomic load.
    static FX_REQUEST: Mutex<Cell<(u8, u16)>> = Mutex::new(Cell::new((0, 0)));
    static FX_GENERATION: AtomicU32 = AtomicU32::new(0);

    /// Request effect `code` at `value`. `false` for an unknown code, changing nothing.
    #[cfg(feature = "log-usb")]
    pub fn select_fx(code: u8, value: u16) -> bool {
        if asperitas_dsp::replay_fx::FxKind::from_code(code).is_none() {
            return false;
        }
        cortex_m::interrupt::free(|cs| FX_REQUEST.borrow(cs).set((code, value)));
        FX_GENERATION.fetch_add(1, Ordering::Release);
        true
    }

    /// The host's overrides of the patch's knobs: 0 filter cutoff, 1 output gain.
    static KNOB: Mutex<RefCell<KnobOverride<KNOB_COUNT>>> =
        Mutex::new(RefCell::new(KnobOverride::new()));

    /// Override knob `knob` with the wire value `value` (`0..=65535` spans `0.0..=1.0`). `false`
    /// for a knob the patch does not have.
    #[cfg(feature = "log-usb")]
    pub fn override_set(knob: u8, value: u16) -> bool {
        let v = f32::from(value) / f32::from(u16::MAX);
        cortex_m::interrupt::free(|cs| KNOB.borrow(cs).borrow_mut().set(usize::from(knob), v))
    }

    /// Return knob `knob` to its default. `false` for a knob the patch does not have.
    #[cfg(feature = "log-usb")]
    pub fn override_release(knob: u8) -> bool {
        if usize::from(knob) >= KNOB_COUNT {
            return false;
        }
        cortex_m::interrupt::free(|cs| KNOB.borrow(cs).borrow_mut().release(usize::from(knob)));
        true
    }

    /// Render one block: the generator, then the effect, applying a pending `FX` first.
    ///
    /// `fx_seen` is the generation this stage last applied. The callback's cost beyond the bare
    /// generator is one relaxed atomic load in bypass; with the patch selected it adds 32 effect
    /// ticks per block. No await, no flash access, no allocation; the
    /// critical sections are a handful of cycles and only run when a request is pending or an
    /// effect is selected.
    pub fn render<G: Processor>(chain: &mut FxChain<G>, fx_seen: &mut u32, block: &mut [Frame]) {
        let generation = FX_GENERATION.load(Ordering::Acquire);
        if generation != *fx_seen {
            *fx_seen = generation;
            let (code, value) = cortex_m::interrupt::free(|cs| FX_REQUEST.borrow(cs).get());
            chain.select(code, value);
        }
        chain.process_block_with(block, |base| {
            // Upstream of smoothing, as `main` does.
            cortex_m::interrupt::free(|cs| KNOB.borrow(cs).borrow().merge(base))
        });
    }
}

#[cfg(feature = "stim-excerpt")]
mod excerpt_play {
    use super::{emit_console, flash_of, FLASH};
    use asperitas_dsp::processor::{Frame, Processor};
    use asperitas_dsp::stimulus::Stimulus;
    use asperitas_logging::{error, info};
    use asperitas_replay::{
        FillClaim, PassReport, ReplayCore, ASSUMED_SCHEDULING_LATENCY_MICROS, BLOCK_SAMPLES,
        BYTES_PER_SAMPLE,
    };
    use asperitas_wire::console;
    use asperitas_wire::excerpt::{self, SlotHeader, HEADER_LEN};
    use core::fmt::Write as _;
    use cortex_m::peripheral::SCB;

    /// The slot this build plays: `ASP_RIG_EXCERPT_SLOT` when set, otherwise 0.
    ///
    /// Read with `option_env!`, so rustc tracks the variable and changing it rebuilds.
    pub const SLOT: u8 = match option_env!("ASP_RIG_EXCERPT_SLOT") {
        Some(text) => parse_slot(text),
        None => 0,
    };

    /// Parse a slot number at compile time. Anything but a whole number below `SLOT_COUNT` fails the
    /// build, naming the variable, instead of falling back to a slot nobody asked for.
    const fn parse_slot(text: &str) -> u8 {
        let bytes = text.as_bytes();
        assert!(!bytes.is_empty(), "ASP_RIG_EXCERPT_SLOT is set but empty");
        let mut slot = 0u32;
        let mut i = 0;
        while i < bytes.len() {
            assert!(
                bytes[i].is_ascii_digit(),
                "ASP_RIG_EXCERPT_SLOT must be a whole slot number"
            );
            slot = slot * 10 + (bytes[i] - b'0') as u32;
            // Bail before the accumulator can overflow on a long digit string.
            assert!(
                slot < excerpt::SLOT_COUNT as u32,
                "ASP_RIG_EXCERPT_SLOT is not below excerpt::SLOT_COUNT"
            );
            i += 1;
        }
        slot as u8
    }

    const _: () = assert!(parse_slot("0") == 0);
    const _: () = assert!(parse_slot("13") == 13);

    /// Flash address of the slot's first PCM byte.
    const PCM_BASE: u32 = match excerpt::slot_pcm_base(SLOT) {
        Some(base) => base,
        None => panic!("SLOT is validated by parse_slot"),
    };

    /// How long the refill task sleeps when no half is free to claim, in milliseconds.
    ///
    /// Chosen against the staging table in `replay`: a half plays for 85 ms, and the table budgets
    /// `ASSUMED_SCHEDULING_LATENCY_MICROS` (10 ms) for the refill task to get to run. A poll period
    /// of at most half of that keeps the poll's own contribution to the latency inside the budget it
    /// already assumed, and a wake every 2 ms on an otherwise idle thread executor is negligible.
    const REFILL_POLL_MS: u64 = 2;
    const _: () = assert!(
        REFILL_POLL_MS * 1_000 <= ASSUMED_SCHEDULING_LATENCY_MICROS as u64 / 2,
        "the refill poll period eats the scheduling latency the staging table budgets"
    );

    /// The staging ping-pong. See "Where the staging lives" above for what region it is in.
    ///
    /// `.replay_bss` is NOLOAD in both link layouts, so nothing initialises it: [`zero_staging`] must
    /// run before the first use. All-zero is a valid idle state (`PHASE_IDLE`, `HalfState::Empty`).
    #[link_section = ".replay_bss"]
    static REPLAY: ReplayCore = ReplayCore::new();

    /// Zero the staging. Called once from `main`, before [`load`] and before any other task exists.
    pub fn zero_staging() {
        // SAFETY: called once at boot, single-threaded, before the SAI interrupt or any refill can
        // touch the core. `addr_of!` takes the address without making a reference. The core is
        // plain integers and atomics for which all-zero bytes are valid.
        unsafe {
            core::ptr::write_bytes(core::ptr::addr_of!(REPLAY).cast_mut(), 0, 1);
        }
    }

    /// Whether the pass has been started and has not yet completed.
    ///
    /// `load` starts the pass before anything else can ask, so "no report yet" means "open". Only the
    /// install gate asks, and that exists only with the console.
    #[cfg(feature = "log-usb")]
    pub fn pass_open() -> bool {
        REPLAY.finish().is_none()
    }

    /// One cache line, aligned like one, for the header read's destination.
    #[repr(align(32))]
    struct Line([u8; 32]);
    const _: () = assert!(HEADER_LEN <= 32);

    /// Discard any cached copy of `buf`'s lines after a DMA wrote it. See the section comment.
    fn invalidate_after_dma(scb: &mut SCB, buf: &mut [u8]) {
        if !SCB::dcache_enabled() {
            return;
        }
        let addr = buf.as_ptr() as usize;
        debug_assert!(addr.is_multiple_of(32), "DMA buffer is not line aligned");
        // SAFETY: `buf` starts on a line and every caller passes a buffer inside a 32-byte-aligned
        // region whose length is a multiple of 32 (`Line`, and a staging half), so rounding the
        // length up stays inside that region and invalidates no neighbour's lines. Nothing in it is
        // dirty: the CPU never writes these bytes.
        unsafe { scb.invalidate_dcache_by_address(addr, buf.len().next_multiple_of(32)) };
    }

    /// Read the claimed half's source range from QSPI, invalidate its cache lines, publish it.
    async fn fill_half(scb: &mut SCB, mut claim: FillClaim<'_>) {
        let address = PCM_BASE + claim.source().start;
        assert!(
            address >= excerpt::slot_base(SLOT).unwrap_or(u32::MAX),
            "rig: replay read below its slot"
        );
        let buffer = claim.buffer();
        {
            // Held across this one read and nothing else, as `FLASH`'s rule says.
            let mut guard = FLASH.lock().await;
            flash_of(&mut guard).read_async(address, buffer).await;
        }
        invalidate_after_dma(scb, buffer);
        claim.finish();
    }

    /// Read and validate the slot header, start the pass and stage both halves.
    ///
    /// Everything that can make a slot unplayable is decided here, before audio exists.
    pub async fn load(scb: &mut SCB) -> Result<ExcerptPlayer, console::PlayRefusal> {
        let mut line = Line([0; 32]);
        {
            let mut guard = FLASH.lock().await;
            let base = excerpt::slot_base(SLOT).expect("SLOT is validated by parse_slot");
            flash_of(&mut guard).read_async(base, &mut line.0).await;
        }
        invalidate_after_dma(scb, &mut line.0);
        let header = SlotHeader::decode(&line.0).map_err(|why| match why {
            // Blank flash reads 0xFF and an install in progress erased the header sector first: both
            // are "nothing here". Everything else is something that is not a header.
            excerpt::HeaderError::Magic => console::PlayRefusal::Empty,
            _ => console::PlayRefusal::Invalid,
        })?;
        REPLAY
            .start(header.pcm_bytes())
            .expect("rig: a validated header has an even length, and no pass is running at boot");
        // Both halves, so the callback's first block finds one `Ready` (see "Boot order").
        while let Some(claim) = REPLAY.claim_fill() {
            fill_half(scb, claim).await;
        }
        Ok(ExcerptPlayer { header })
    }

    /// Refill halves as the callback drains them; after the pass, report it once.
    ///
    /// Returns after emitting `EXCPLAY`. Polls rather than waking on a signal: a wake from the
    /// callback would be a lock or a pender call at audio priority, and the poll's cost is in
    /// `REFILL_POLL_MS`.
    pub async fn run(scb: &mut SCB, want: u16) {
        loop {
            if let Some(report) = REPLAY.finish() {
                emit_pass(&report, want);
                return;
            }
            match REPLAY.claim_fill() {
                Some(claim) => fill_half(scb, claim).await,
                None => embassy_time::Timer::after_millis(REFILL_POLL_MS).await,
            }
        }
    }

    /// Send the one `EXCPLAY` record for a completed pass.
    fn emit_pass(report: &PassReport, want: u16) {
        let play = console::ExcerptPlay {
            slot: SLOT,
            bytes: report.samples.saturating_mul(BYTES_PER_SAMPLE as u32),
            crc16: report.crc16,
            want,
            underruns: report.underruns,
            clean: report.is_clean(),
        };
        let mut body = [0u8; console::BODY_WINDOW];
        let n = console::exc_play_body(&play, &mut body);
        info!(
            "rig: replay pass done: slot {} crc16 {:04x} want {:04x} underruns {}",
            play.slot, play.crc16, play.want, play.underruns
        );
        if play.crc16 != play.want {
            error!("rig: replay CRC differs from the stored one");
        }
        emit_console(&body[..n]);
    }

    /// The generator the audio task holds: a thin front for the shared [`REPLAY`] core.
    pub struct ExcerptPlayer {
        header: SlotHeader,
    }

    impl ExcerptPlayer {
        /// The CRC the slot header stored: the pass's `want`.
        pub fn want_crc16(&self) -> u16 {
            self.header.pcm_crc16()
        }
    }

    impl Processor for ExcerptPlayer {
        type Params = ();

        fn set_sample_rate(&mut self, _hz: f32) {
            // The excerpt is 48 kHz by construction (`excerpt::WAV_SAMPLE_RATE`), checked at install.
        }

        fn set_params(&mut self, _params: &()) {}

        /// Silence: this source is block-granular, because the CRC and the half hand-off are both
        /// defined per 32-sample block. Present only because the trait requires it.
        fn tick(&mut self, _input: Frame) -> Frame {
            [0.0; 2]
        }

        /// See "When the pass runs": the pass was started at boot.
        fn reset(&mut self) {}

        /// The whole audio-path cost of replay: `take_block` and a sample conversion. No QSPI, no
        /// flash mutex, no await, no allocation. Samples convert as `s / 32768` into both channels.
        fn process_block(&mut self, output: &mut [Frame]) {
            let mut block = [0i16; BLOCK_SAMPLES];
            let _ = REPLAY.take_block(&mut block);
            for (frame, sample) in output.iter_mut().zip(block) {
                let value = f32::from(sample) / 32768.0;
                *frame = [value, value];
            }
        }
    }

    /// A `fmt::Write` that stops at the end of its buffer, like the dsp crate's private one.
    struct Fill<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl core::fmt::Write for Fill<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let room = self.buf.len() - self.pos;
            let n = s.len().min(room);
            self.buf[self.pos..self.pos + n].copy_from_slice(&s.as_bytes()[..n]);
            self.pos += n;
            Ok(())
        }
    }

    impl Stimulus for ExcerptPlayer {
        /// `name=excerpt sample_rate_hz=48000 slot=<n> tag=<name> bytes=<n> crc16=<4 hex>`: at most
        /// 83 bytes against `RIGGEN`'s 160. No level: the excerpt's level is its own.
        fn describe(&self, out: &mut [u8]) -> usize {
            let mut w = Fill { buf: out, pos: 0 };
            let tag = core::str::from_utf8(self.header.name().as_bytes()).unwrap_or("?");
            let _ = write!(
                w,
                "name=excerpt sample_rate_hz={} slot={} tag={} bytes={} crc16={:04x}",
                excerpt::WAV_SAMPLE_RATE,
                SLOT,
                tag,
                self.header.pcm_bytes(),
                self.header.pcm_crc16(),
            );
            w.pos
        }

        /// The excerpt plays once, so the player starts it inside the capture and says where.
        fn one_shot_samples(&self) -> Option<u32> {
            Some(self.header.pcm_bytes() / BYTES_PER_SAMPLE as u32)
        }
    }
}

#[cfg(feature = "stim-excerpt")]
use excerpt_play::ExcerptPlayer;

/// Whether a replay pass is open, for the install gate. Always false without `stim-excerpt`.
#[cfg(feature = "log-usb")]
fn replay_pass_open() -> bool {
    #[cfg(feature = "stim-excerpt")]
    {
        excerpt_play::pass_open()
    }
    #[cfg(not(feature = "stim-excerpt"))]
    {
        false
    }
}

/// Refuse to play: say why, show red, and keep serving the console so the record can leave.
///
/// `boot_halt` spins and so would strand the record in the pipe; this keeps the drain and the LED
/// tasks running and starts no audio, which is the point - a slot with nothing playable is a
/// configuration mistake to surface, not a reason to play silence and look healthy.
#[cfg(feature = "stim-excerpt")]
async fn halt_unplayable(why: console::PlayRefusal, console: asperitas_logging::Console) -> ! {
    error!(
        "rig: excerpt slot {} is not playable ({}); not starting audio",
        excerpt_play::SLOT,
        why.as_str()
    );
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::exc_play_refused_body(excerpt_play::SLOT, why, &mut body);
    emit_console(&body[..n]);
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Panicked);
    embassy_futures::select::select(console.run(), asperitas_logging::led::blink_task()).await;
    #[allow(clippy::empty_loop)]
    loop {}
}

/// The audio callback, run by the shared audio executor at SAI1's priority.
///
/// `generator` is a sole `&'static mut`, so the callback needs no lock. That is deliberate - the
/// alternative, reaching the generator through `cortex_m::interrupt::free`, puts a PRIMASK
/// critical section on the highest-rate path in the firmware, where the whole point of the
/// interrupt executor is to avoid one.
///
/// ### Timer-slot budget: 6 named users of 8 slots, and none of them here
///
/// `generic-queue-8` is fixed by daisy-embassy's own `embassy-time` dependency; adding a
/// `generic-queue-N` here would collide on `const QUEUE_SIZE` and fail to compile, so eight is
/// the whole budget. Its users: boot 1 (DWT calibration, the codec's 2 ms startup delay,
/// `slow-boot`'s linger), `CAPSTAT` reporting ticker 1, LED blink 1, capture timeline 1 (arm delay,
/// window deadline, early-stop poll), dump retry 1, excerpt install 1 (the flash driver's
/// `with_timeout` on every erase and page program), USB drain 0 - deliberately zero, see `usb.rs`'s
/// parking drain. Six, as a ceiling. In practice fewer are held at once: slots are keyed by
/// *waker*, every one of those six runs inside `main`'s task (boot before the select, the rest
/// joined inside it), and so they collapse onto that task's single slot. Overflow does not panic:
/// `queue_generic.rs` evicts the furthest-out timer and wakes it early, so a blown budget shows up
/// as a mistimed `CAPSTAT` rather than an error. Count a new timer against the six above, and a
/// new *task* with a timer against the eight.
///
/// The audio task takes none of them: after `start_callback` is awaited there is nothing left to
/// await, which is the requirement rather than an observation. A task that awaited between blocks
/// would be re-spawned through the pender and turn the deterministic wake (SAI1 fired) into a
/// queueing delay.
struct RigAudio {
    generator: &'static mut ActiveGenerator,
    producer: Producer,
    /// Silence until the generator plays. A one-shot generator is held back until the capture's
    /// first callback: started at boot, it would be two seconds (ARM_DELAY_MS) through before the
    /// recording began, and the analysis needs it whole. A periodic one plays from the first
    /// callback, because any stretch of it is as good as any other.
    frames_out: [Frame; BLOCK_LENGTH],
    playing: bool,
    /// The `FX` generation last applied (see `fx_ctl::render`).
    fx_seen: u32,
}

impl RigAudio {
    fn new(generator: &'static mut ActiveGenerator, ring: Ring) -> Self {
        let playing = generator.one_shot_samples().is_none();
        Self {
            generator,
            producer: Producer {
                ring,
                callbacks_in_block: 0,
                last_entry: None,
            },
            frames_out: [Frame::default(); BLOCK_LENGTH],
            playing,
            fx_seen: 0,
        }
    }
}

impl AudioProcessor for RigAudio {
    fn process(&mut self, input: &[u32], output: &mut [u32]) {
        // Entry time first, exit time last, so `max_block_us` prices everything the callback
        // does, capture included.
        let entry = DWT::cycle_count();

        // Capture what came back first: the input lane, after the self-loopback cable. Before
        // the render, because whether this callback was captured is what starts a one-shot
        // generator. The order costs nothing: the output buffer is not played until the DMA
        // reaches it next period, whichever half of the callback fills it.
        let captured = self.producer.on_callback(entry, input);
        if let (false, Some(at)) = (self.playing, captured) {
            // This callback's output is the stimulus's first sample, and its input is being
            // captured at `at`. Written before the release that publishes it.
            self.generator.reset();
            STIM_BLOCK.store(at.block, Ordering::Relaxed);
            STIM_CALLBACK.store(at.callback as u32, Ordering::Relaxed);
            STIM_STARTED.store(true, Ordering::Release);
            self.playing = true;
        }

        // Render site, mirroring `main.rs`: stimulus -> [processor slot] -> encode_block.
        // The processor slot is empty by design and is where a measured effect will hang.
        if self.playing {
            fx_ctl::render(self.generator, &mut self.fx_seen, &mut self.frames_out);
        }
        audio::encode_block(&self.frames_out, output);
        self.producer.finish(entry);
    }
}

static RIG_AUDIO: StaticCell<RigAudio> = StaticCell::new();

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
        blocks: asperitas_wire::capture::RING_BLOCKS as u32,
        block_bytes: asperitas_wire::capture::RING_BLOCK_BYTES as u32,
        bytes_per_s: asperitas_wire::capture::BYTES_PER_SECOND as u32,
        capsec_us: asperitas_wire::capture::ring_duration_micros() as u32,
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
// Boot
// ---------------------------------------------------------------------------

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

    // The flash's DMA channel, taken before the macro: `new_daisy_board!` moves the fields it uses
    // out of `p` and leaves `MDMA_CH0` behind, but `p` is gone from view after this line.
    let flash_dma = p.MDMA_CH0;

    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals - `asperitas_logging::init` steals them directly via Peri::steal(). Same
    // invariant as `main.rs`: reading this field would create a second handle for the same
    // physical peripheral.
    let _ = board.usb_peripherals;

    // Init RGB LED - single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Bring up logging. Records logged before this call are discarded.
    let console = asperitas_logging::init(RigUsbIrqs);
    info!("rig booting");

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

    // The QSPI flash, async-only, then its two vectors below the audio executor: see "Excerpt install"
    // for both choices. Before audio starts, because `build_async` resets the chip with the driver's
    // blocking status wait; that is the only blocking flash wait in the image, and it runs once,
    // here, with nothing yet to starve.
    let flash = board.flash.build_async(flash_dma, RigFlashIrqs);
    interrupt::QUADSPI.set_priority(Priority::P7);
    interrupt::MDMA.set_priority(Priority::P7);
    *FLASH.lock().await = Some(flash);

    // Build the generator before audio starts. `ExponentialSweep`'s constructor peak-normalises
    // by scanning its whole 384,000-sample record, which is milliseconds at 48 kHz but belongs
    // here rather than in a callback under no circumstances. The log line goes first because a
    // board that died mid-scan would otherwise look idle.
    info!("rig: building stimulus (sweep builds scan their whole record)");
    //
    // `stim-excerpt` builds it from the slot instead: the header read, the pass start and both halves'
    // first fill happen here, before `prepare_interface`, so the audio start is not delayed by flash.
    #[cfg(feature = "stim-excerpt")]
    excerpt_play::zero_staging();
    #[cfg(feature = "stim-excerpt")]
    let player = match excerpt_play::load(&mut cp.SCB).await {
        Ok(player) => player,
        Err(why) => halt_unplayable(why, console).await,
    };
    #[cfg(feature = "stim-excerpt")]
    let want_crc = player.want_crc16();
    #[cfg(feature = "stim-excerpt")]
    let generator = GENERATOR.init(chain_of(player));
    #[cfg(not(feature = "stim-excerpt"))]
    let generator = GENERATOR.init(chain_of(BareGenerator::default()));
    generator.set_sample_rate(SAMPLE_RATE_HZ);

    // Prepare the audio interface (SAI + codec init + DMA buffers)
    let interface = board
        .audio_peripherals
        .prepare_interface(AudioConfig { fs: RIG_FS })
        .await;

    // Linger on the pre-init red so it can actually be seen. Everything above this point takes a
    // few milliseconds, so without the delay red -> green reads as "always green" and the two
    // stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    // Running now; `audio::start` moves the LED to Panicked and records the reason in
    // `audio::AUDIO_EXIT` (which `CAPSTAT` reports) if the interface then fails to start.
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Say what is about to play, and what the ring can hold, once, before anything plays it.
    emit_descriptors(generator, time_base);
    emit_capmax();
    // Read before the generator moves into the audio task: the timeline needs it to check the fit.
    let one_shot = generator.one_shot_samples();

    // P6 sits below the SAI DMA streams and the embassy-time driver (TIM5), which is the whole
    // argument that the callback can run without a critical section: the transfers it depends on
    // finish ahead of it rather than underneath it. The DMA side is P0 because `Config::default()`
    // ships `dma_interrupt_priority: Priority::P0` (embassy-stm32 0.6.0 `src/lib.rs:362`), and the
    // direction is required, not incidental: the DMA ISR is what pends SAI1, so it must outrank
    // the executor it wakes. `audio::start` sets the priority; the numbers are logged below as a
    // reading of the NVIC, not as a restatement of this comment.
    //
    // Descriptors went out above because rendering them on the first SAI1 interrupt is what the
    // `emit_descriptors` comment rules out.
    let audio_processor = RIG_AUDIO.init(RigAudio::new(generator, ring));
    audio::start(interface, audio_processor).await;

    // Effective NVIC priorities, read back after the vector was unmasked. Lower numbers win on
    // Cortex-M (P0 is the highest, seven implemented bits, no subpriority on this part), so the
    // claim being checked is `tim5 < sai1` and `dma1_* < sai1`, and the opposite for the flash:
    // `sai1 < quadspi` and `sai1 < mdma`.
    info!(
        "rig: nvic priority sai1={} dma1_stream0={} dma1_stream1={} tim5={} quadspi={} mdma={}",
        NVIC::get_priority(interrupt::SAI1),
        NVIC::get_priority(interrupt::DMA1_STREAM0),
        NVIC::get_priority(interrupt::DMA1_STREAM1),
        NVIC::get_priority(interrupt::TIM5),
        NVIC::get_priority(interrupt::QUADSPI),
        NVIC::get_priority(interrupt::MDMA),
    );

    // Thread mode is left to what must not run at audio priority: the console drain, which is also
    // the reporting task that paces `STATUS`, the boot LED, `CAPSTAT`, the capture timeline with
    // its dump, and the excerpt installer. None of them has a deadline.
    //
    // The inbound reader is switched on here and only here, before `run()` is first polled: rig is
    // the one binary with a consumer for it, and every other binary keeps EP OUT unread.
    #[cfg(feature = "log-usb")]
    let install_fut = run_install(asperitas_logging::usb::enable_inbound());
    // No console, no inbound path, nothing to install from, so this never wakes.
    #[cfg(not(feature = "log-usb"))]
    let install_fut = core::future::pending::<()>();
    let console_fut = console.run();
    let led_fut = asperitas_logging::led::blink_task();
    // The timeline finishes after its dump; the reporter and the installer never do, so the join
    // never completes.
    //
    // The replay refill and its one report finish after the pass, so they do not end the join either.
    #[cfg(feature = "stim-excerpt")]
    let replay_fut = excerpt_play::run(&mut cp.SCB, want_crc);
    #[cfg(not(feature = "stim-excerpt"))]
    let replay_fut = core::future::ready(());
    let rig_fut = embassy_futures::join::join4(
        report_capstat(time_base),
        run_capture(ring, time_base, one_shot),
        install_fut,
        replay_fut,
    );

    // All three loop forever, so reaching past the select means one of them returned: the thread
    // executor parks and the audio interrupt keeps playing, which is a machine still making sound
    // and reporting nothing about it. Halt rather than become that.
    embassy_futures::select::select3(console_fut, led_fut, rig_fut).await;
    error!("rig: console, LED or capture reporting returned; stopping");
    #[allow(clippy::empty_loop)]
    loop {}
}
