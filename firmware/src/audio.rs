//! The audio path every binary that plays sound shares: the codec word conversion, the SAI1
//! interrupt executor that owns the callback, and `start`, which brings the interface up and
//! hands it to a caller-supplied processor.
//!
//! The callback must run within about 1.3 ms of every DMA half-transfer (two 32-frame halves at
//! 48 kHz), or the SAI ring overruns and `start_callback` returns `Overrun`. On the thread
//! executor that deadline would be shared with blocking ADC reads and the USB console, so the
//! callback gets its own interrupt executor above thread mode and below the DMA streams (P6 vs
//! P0), which makes the deadline the callback's own.

use core::sync::atomic::{AtomicU32, Ordering};

use asperitas_dsp::processor::Frame;
use asperitas_logging::led::{set_global_state, LedState};
use asperitas_logging::{error, info};
use daisy_embassy::audio::{Idle, Interface, Running};
use daisy_embassy::hal;
use daisy_embassy::hal::interrupt::{InterruptExt, Priority};
use embassy_executor::InterruptExecutor;

/// Frames per DMA half-transfer, the block size of daisy-embassy's audio callback.
pub const BLOCK_LENGTH: usize = 32;

/// Why the audio path ended, for a debug probe to read from [`AUDIO_EXIT`]. The numeric values
/// are the encoding a probe sees: 0 still running, 1 `NotATransmitter`, 2 `NotAReceiver`,
/// 3 `Overrun`, with [`START_FAILED`] OR-ed in when the interface failed to start rather than
/// while running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum AudioExit {
    Running = 0,
    NotATransmitter = 1,
    NotAReceiver = 2,
    Overrun = 3,
}

/// Bit set in [`AUDIO_EXIT`] when the failure happened while starting the interface.
pub const START_FAILED: u32 = 0x10;

impl AudioExit {
    fn from_sai(e: hal::sai::Error) -> Self {
        match e {
            hal::sai::Error::NotATransmitter => Self::NotATransmitter,
            hal::sai::Error::NotAReceiver => Self::NotAReceiver,
            hal::sai::Error::Overrun => Self::Overrun,
        }
    }

    fn record(self, start_failed: bool) {
        let code = self as u32 | if start_failed { START_FAILED } else { 0 };
        AUDIO_EXIT.store(code, Ordering::Relaxed);
    }
}

/// Exit reason as an [`AudioExit`] code (see the type), readable by a probe.
#[used]
pub static AUDIO_EXIT: AtomicU32 = AtomicU32::new(AudioExit::Running as u32);

/// Callbacks run since boot, for a probe: how far the audio loop got before it ended.
#[used]
pub static AUDIO_BLOCKS: AtomicU32 = AtomicU32::new(0);

/// What runs once per audio block. `input` and `output` are interleaved stereo codec words,
/// `BLOCK_LENGTH * 2` long.
pub trait AudioProcessor {
    fn process(&mut self, input: &[u32], output: &mut [u32]);
}

/// Convert u32 codec samples to stereo frames on the stack.
///
/// TAC5242 delivers 32-bit left-justified signed PCM in u32 containers. Input length must be
/// `BLOCK_LENGTH * 2` (interleaved stereo). Output values are normalized to [-1.0, 1.0].
pub fn decode_block(input: &[u32], output: &mut [Frame; BLOCK_LENGTH]) {
    for (words, frame) in input.chunks_exact(2).zip(output.iter_mut()) {
        let left = (words[0] as i32) as f32 / i32::MAX as f32;
        let right = (words[1] as i32) as f32 / i32::MAX as f32;
        frame[0] = left.clamp(-1.0, 1.0);
        frame[1] = right.clamp(-1.0, 1.0);
    }
}

/// Convert processed stereo frames back to u32 codec samples, clamped to [-1.0, 1.0] and scaled
/// to the 32-bit signed range.
pub fn encode_block(input: &[Frame; BLOCK_LENGTH], output: &mut [u32]) {
    for (frame, words) in input.iter().zip(output.chunks_exact_mut(2)) {
        let left = (frame[0].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        let right = (frame[1].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        words[0] = left;
        words[1] = right;
    }
}

/// Executor that owns the audio callback, driven by the SAI1 interrupt.
static AUDIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

/// Poll the audio executor; the body of the SAI1 vector.
///
/// # Safety
/// Call only from the SAI1 interrupt handler, and only after [`start`] has unmasked it.
#[doc(hidden)]
pub unsafe fn on_sai1_interrupt() {
    unsafe { AUDIO_EXECUTOR.on_interrupt() }
}

/// Define the SAI1 vector that drives the audio executor. Exactly one binary-level definition
/// may exist per image, and the vector must be written by hand because `bind_interrupts!` and
/// `#[interrupt]` cannot both name a vector. It is a macro rather than a lib item so that a
/// binary with its own SAI1 handler can still link this crate.
#[macro_export]
macro_rules! audio_sai1_vector {
    () => {
        const _: () = {
            use $crate::audio::interrupt;

            #[interrupt]
            unsafe fn SAI1() {
                unsafe { $crate::audio::on_sai1_interrupt() }
            }
        };
    };
}

#[doc(hidden)]
pub use daisy_embassy::hal::interrupt;

#[embassy_executor::task]
async fn audio_task(
    mut interface: Interface<'static, Running>,
    processor: &'static mut (dyn AudioProcessor + Send),
) {
    let outcome = interface
        .start_callback(|input, output| {
            AUDIO_BLOCKS.fetch_add(1, Ordering::Relaxed);
            processor.process(input, output);
        })
        .await;
    // `Ok` is `Infallible`, so this is the SAI hardware error.
    let Err(e) = outcome;
    fail(AudioExit::from_sai(e), false);
}

/// The one failure policy: leave the reason where a probe can read it, say so on the LED and in
/// the log, and return. Nothing here halts, so thread mode keeps serving the console.
fn fail(reason: AudioExit, start_failed: bool) {
    reason.record(start_failed);
    set_global_state(LedState::Panicked);
    error!(
        "audio ended: {:?} start_failed={}",
        reason as u32, start_failed
    );
}

/// Start the interface and run `processor` on every block from the SAI1 interrupt executor.
///
/// Returns once the audio task is spawned. If the interface fails to start, the reason is
/// recorded in [`AUDIO_EXIT`], the LED goes to `Panicked`, and this returns without audio.
pub async fn start(
    interface: Interface<'static, Idle>,
    processor: &'static mut (dyn AudioProcessor + Send),
) {
    let interface = match interface.start_interface().await {
        Ok(iface) => iface,
        Err(e) => {
            fail(AudioExit::from_sai(e), true);
            return;
        }
    };
    info!("Audio interface ready");

    interrupt::SAI1.set_priority(Priority::P6);
    let spawner = AUDIO_EXECUTOR.start(interrupt::SAI1);
    spawner.spawn(audio_task(interface, processor).expect("failed to spawn audio task"));
}

// ---------------------------------------------------------------------------
// DWT time base
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
pub fn trace_enabled() -> bool {
    // Safety: DEMCR is a memory-mapped control register present on every Cortex-M7, and reading
    // it has no side effects.
    unsafe { core::ptr::read_volatile(DEMCR as *const u32) & DEMCR_TRCENA != 0 }
}

/// What a duration in cycles is divided by, and how it knows.
#[derive(Clone, Copy)]
pub struct TimeBase {
    /// Core clock in Hz used for conversion.
    pub cpu_hz: u32,
    /// Whole cycles per microsecond, rounded to nearest.
    ///
    /// What every reported duration is divided by (the caller's cycles-to-microseconds conversion); carried here from the one place
    /// that knows how the clock was settled rather than recomputed at each call site.
    pub cycles_per_us: u32,
}

/// Enable DWT's cycle counter, prove it counts, and settle on a cycles-per-microsecond.
///
/// Returns `None` when the counter cannot be made to run. Callers must treat that as fatal:
/// every duration reported is a cycle delta, and against a counter that stays at zero they
/// all read as zero, which is indistinguishable from a device fast enough to not matter.
///
/// `declared_hz` is the figure the clock tree computed from `default_rcc()`'s dividers. It agrees
/// with the datasheet maximum rather than with anything measured, and `rcc::clocks()` reports the
/// same derived number, so agreement between the two below is consistency, not confirmation.
pub async fn measure_time_base(
    cp: &mut cortex_m::peripheral::Peripherals,
    declared_hz: Option<u32>,
) -> Option<TimeBase> {
    if !cortex_m::peripheral::DWT::has_cycle_counter() {
        error!("this core has no DWT cycle counter; refusing to report durations");
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
            "cycle counter unavailable (TRCENA={} CYCCNTENA={} count {} -> {})",
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
        error!("calibration window produced nothing (us={elapsed_us}, cycles={cycles})");
        return None;
    }
    let measured_hz = (cycles * 1_000_000 / elapsed_us) as u32;

    // Publish the declared figure when the tree can state it in whole megahertz and the two agree
    // within a percent; otherwise fall back to the measurement and say so.
    //
    // Declared wins when it can, because the measurement is quantised by the tick rate
    // (`tick-hz-32_768`, one tick per 30.517 us). Over 200 ms that is +/- 0.015 %, which is
    // enough to throw away a digit of a 480 MHz quotient - 479 rather than 480, a 0.2 % error
    // paid on every duration reported, against a gate that only starts caring at 1 %.
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
                "measured clock {measured_hz} Hz disagrees with declared {declared} Hz by over {CALIBRATION_TOLERANCE_PCT}%; using the measurement"
            );
            (measured_hz, "measured")
        }
        None => {
            info!("declared clock unusable ({declared_hz:?}); using the measurement");
            (measured_hz, "measured")
        }
    };

    info!(
        "time base {cpu_hz} Hz ({why}); declared {:?}, measured {measured_hz}; cycles/us {}; TRCENA={} CYCCNTENA={}",
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
