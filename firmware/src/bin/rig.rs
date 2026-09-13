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
//! 3. The DSP chain is a stimulus generator chosen by cargo feature, and the input frame is
//!    ignored: rig measures the box, not the instrument plugged into it.
//!
//! Capture into the SDRAM ring, the dump writer and the rate gates arrive in
//! TASK-038.03.02.04, which extends this file rather than rewriting it.

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
use asperitas_logging::{console, error, info};
use cortex_m::peripheral::NVIC;
use daisy_embassy::audio::{Interface, Running};
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
/// `prepare_interface(Default::default())` selects `Fs::Fs48000`, and the capture geometry in
/// `asperitas-logging::capture` prices itself at 48 kHz mono 16-bit. If those ever disagree the
/// measurement is nonsense, so one of them has to be the source of truth; the capture format is
/// the one the host parses.
const SAMPLE_RATE_HZ: f32 = asperitas_logging::capture::SAMPLE_RATE_HZ as f32;

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
/// binds SAI1 today: daisy-embassy's `AudioIrqs` binds the SAI DMA stream vectors, which are
/// separate interrupts, so the vector is free and this definition is legal. Do not "tidy" it
/// into a `bind_interrupts!` block.
#[interrupt]
unsafe fn SAI1() {
    // Safety: called from the SAI1 handler and nowhere else, and only after `start()` has
    // initialised the executor - `start()` is what unmasks this vector, so no SAI1 interrupt can
    // reach here before then.
    unsafe { AUDIO_EXECUTOR.on_interrupt() }
}

/// Block size used by daisy-embassy's audio callback.
const BLOCK_LENGTH: usize = 32;

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
/// Two definitions rather than a `cfg` at each call site, because the call sites are about to
/// multiply (CAPSTAT, CAPMAX, DUMPEND land in TASK-038.03.02.04) and ten copies of
/// `#[cfg(feature = "log-usb")]` is ten ways to forget one.
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

/// The audio callback, running on [`AUDIO_EXECUTOR`] at SAI1's priority.
///
/// `generator` is a sole `&'static mut`: the only other handle was consumed by the move through
/// `spawn`, so the callback needs no lock. That is deliberate - the alternative, reaching the
/// generator through `cortex_m::interrupt::free`, puts a PRIMASK critical section on the
/// highest-rate path in the firmware, where the whole point of the interrupt executor is to
/// avoid one.
///
/// ### Timer-slot budget: 5 of 8, and none of them here
///
/// `generic-queue-8` is fixed by daisy-embassy's own `embassy-time` dependency; adding a
/// `generic-queue-N` here would collide on `const QUEUE_SIZE` and fail to compile, so eight is
/// the whole budget and it is spent as follows: reporting ticker 1, LED blink 1, dump retry 1,
/// capture window/deadline 1 (those last two are TASK-038.03.02.04's, reserved now so it does not
/// have to evict anything), boot including the codec's 2 ms startup delay 1, USB drain 0 -
/// deliberately zero, see `usb.rs`'s parking drain. Slots are keyed by *waker*, so every timer
/// pending inside one task collapses onto a single slot; capacity therefore counts "tasks with a
/// pending timer", not `Timer` objects. Overflow does not panic: `queue_generic.rs` evicts the
/// furthest-out timer and wakes it spuriously, which would look like an early deadline rather
/// than a crash. Raise any of these five only against that count.
///
/// This task takes none of them: after `start_callback` is awaited there is nothing left to
/// await, which is the requirement rather than an observation. A task that awaited between blocks
/// would be re-spawned through the pender and turn the deterministic wake (SAI1 fired) into a
/// queueing delay.
#[embassy_executor::task]
async fn audio_task(
    mut interface: Interface<'static, Running>,
    generator: &'static mut ActiveGenerator,
) {
    // Silence handed to the generator because `Processor` is shaped for effects: `tick` takes an
    // input frame and these sources discard it. Nothing here routes the codec's input anywhere,
    // so no microphone reaches the wire regardless of what is plugged into the Pod.
    let frames_in = [Frame::default(); BLOCK_LENGTH];
    let mut frames_out = [Frame::default(); BLOCK_LENGTH];

    let outcome = interface
        .start_callback(|_input, output| {
            // Render site, mirroring `main.rs`: stimulus -> [processor slot] -> encode_block.
            //
            // The processor slot is empty by design and is where a measured effect will hang.
            //
            // TASK-038.03.02.04 splices the capture producer into the gap between
            // `process_block` and `encode_block`, reading the mono lane out of `frames_out`
            // before the encode step turns it into interleaved words.
            generator.process_block(&frames_in, &mut frames_out);

            // Mono lane captured by the producer is the LEFT one: `main.rs`'s `decode_block`
            // reads `words[0]` as left from the same interleaved layout `encode_block` writes,
            // so `words[0]` here is left too. All three generators emit the same value on both
            // channels, so this choice constrains the *input* convention the day rig captures a
            // real instrument; it does not change what these bytes contain.
            encode_block(&frames_out, output);
        })
        .await;

    // Reached only if SAI errors: `start_callback` returns `Result<Infallible, sai::Error>`, and
    // the `Ok` arm cannot be constructed.
    match outcome {
        Ok(never) => match never {},
        Err(_) => error!("rig: audio callback stopped with an SAI error"),
    }
    // Nothing awaits below this point, per the doc comment above. Halting inside the interrupt is
    // a blunt instrument, and it is the one `main.rs` already reaches for: rig with no audio is a
    // box that reports numbers about nothing, and silence is easier to notice than a plausible
    // measurement.
    #[allow(clippy::empty_loop)]
    loop {}
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
        lane: console::MonoLane::Left,
        blocks: asperitas_logging::capture::RING_BLOCKS as u32,
        block_bytes: asperitas_logging::capture::RING_BLOCK_BYTES as u32,
        bytes_per_s: asperitas_logging::capture::BYTES_PER_SECOND as u32,
        capsec_us: asperitas_logging::capture::ring_duration_micros() as u32,
        window_s: asperitas_logging::capture::CAPTURE_WINDOW_SECONDS as u32,
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
    /// Read by the capture producer's block-time and gap measurements, which land in
    /// TASK-038.03.02.04; it is carried here from the one place that knows how the clock was
    /// settled rather than recomputed at each call site.
    #[allow(dead_code)]
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
        .prepare_interface(Default::default())
        .await;

    // Start SAI TX/RX and transition to Running state
    let interface = match interface.start_interface().await {
        Ok(iface) => iface,
        Err(_) => {
            error!("rig: SAI interface failed to start");
            boot_halt();
        }
    };

    info!("Audio interface ready");

    // Linger on the pre-init red so it can actually be seen. Everything above this point takes a
    // few milliseconds, so without the delay red -> green reads as "always green" and the two
    // stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Say what is about to play, once, before anything plays it.
    emit_descriptors(generator, time_base);

    // Priority first, then `start`. `InterruptExecutor::start()` documents that the priority must
    // be set before it and MUST NOT be touched after; setting it later is not merely late, it is
    // undefined, because the executor's pend-from-software assumes the vector's slot is final.
    //
    // P6 sits below the SAI DMA streams and the embassy-time driver (TIM5), which is the whole
    // argument that the callback can run without a critical section: the transfers it depends on
    // finish ahead of it rather than underneath it. The numbers are logged below as a reading of
    // the NVIC, not as a restatement of this comment.
    interrupt::SAI1.set_priority(Priority::P6);
    let audio_spawner = AUDIO_EXECUTOR.start(interrupt::SAI1);
    audio_spawner.spawn(audio_task(interface, generator).expect("failed to spawn audio task"));

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
    // the reporting task that paces `STATUS`, and the boot LED. Neither has a deadline.
    #[cfg(feature = "log-usb")]
    let console_fut = asperitas_logging::usb::run();
    // No drain task exists without the console, and RTT needs no task to exist at all - the
    // probe reads RAM behind our back. `pending()` stands in so this future is written once
    // instead of twice: it never wakes, exactly like the drain task it replaces.
    #[cfg(not(feature = "log-usb"))]
    let console_fut = core::future::pending::<()>();
    let led_fut = asperitas_logging::led::blink_task();

    // Both halves loop forever, so reaching past the select means one of them returned: the
    // thread executor parks and the audio interrupt keeps playing, which is a machine still
    // making sound and reporting nothing about it. Halt rather than become that.
    embassy_futures::select::select(console_fut, led_fut).await;
    error!("rig: console or LED task returned; stopping");
    #[allow(clippy::empty_loop)]
    loop {}
}
