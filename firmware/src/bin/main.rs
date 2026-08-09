#![no_std]
#![no_main]

use core::cell::UnsafeCell;

use asperitas_dsp::filter::OnePoleLowPass;
use asperitas_dsp::gain::Gain;
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_logging::info;
use asperitas_pod::knob::Knobs;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use static_cell::StaticCell;

// Re-export the panic handler from asperitas-logging.
// The #[panic_handler] attribute must be in the binary crate for the linker
// to pick it up, so we define a thin wrapper here.
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    asperitas_logging::panic_handler::handle_panic(info)
}

// Provide _defmt_panic symbol required by embassy-stm32 / embassy-usb's
// internal defmt usage (defmt 0.3.x). This is NOT the Rust panic handler;
// it fires only when a defmt formatter encounters an unrecoverable error.
// No `bkpt()`: with no debug probe attached it escalates to a HardFault
// instead of halting, turning a diagnostic into a silent lockup.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// No-op defmt logger — satisfies linker symbols required by embassy-stm32's
// internal defmt usage. Remove when adding real logging (e.g. defmt-rtt).
//
// NOTE: This block must live in each binary crate, not in a shared lib.
// `#[defmt::global_logger]` is a proc-macro that emits linker symbols only
// when expanded inside the final binary crate; placing it in a lib crate
// causes dead-code elimination to drop the struct (and its generated
// symbols) because nothing references `Logger` by name.
#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn release() {}
    unsafe fn flush() {}
    unsafe fn write(data: &[u8]) {
        let _ = data;
    }
}

bind_interrupts!(pub struct UsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

/// Block size used by daisy-embassy's audio callback.
const BLOCK_LENGTH: usize = 32;

/// Shared knob readings — written by control-surface task, read by audio callback.
///
/// Stored as `[f32; 2]` in static memory behind `UnsafeCell` + `cortex_m::interrupt::free`.
/// Safe on single-core Cortex-M7: the callback runs at fixed priority, the knob
/// task runs in the executor, and interrupt-free sections prevent concurrent access.
struct KnobState {
    inner: UnsafeCell<[f32; 2]>,
}

impl KnobState {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new([0.0, 0.0]),
        }
    }

    /// Write new knob values from the polling task.
    fn write(&self, values: [f32; 2]) {
        unsafe { *self.inner.get() = values };
    }

    /// Read current knob values from the audio callback (inside critical section).
    fn read(&self) -> [f32; 2] {
        unsafe { *self.inner.get() }
    }
}

static KNOB_STATE: StaticCell<KnobState> = StaticCell::new();

/// Convert u32 codec samples to stereo frames on the stack.
///
/// TAC5242 delivers 32-bit left-justified signed PCM in u32 containers.
/// Input length must be `BLOCK_LENGTH * 2` (interleaved stereo).
/// Output is `[Frame; BLOCK_LENGTH]` with values normalized to [-1.0, 1.0].
fn decode_block(input: &[u32], output: &mut [Frame; BLOCK_LENGTH]) {
    for (words, frame) in input.chunks_exact(2).zip(output.iter_mut()) {
        let left = (words[0] as i32) as f32 / i32::MAX as f32;
        let right = (words[1] as i32) as f32 / i32::MAX as f32;
        frame[0] = left.clamp(-1.0, 1.0);
        frame[1] = right.clamp(-1.0, 1.0);
    }
}

/// Convert processed stereo frames back to u32 codec samples.
///
/// Values are clamped to [-1.0, 1.0] and scaled to 32-bit signed range.
fn encode_block(input: &[Frame; BLOCK_LENGTH], output: &mut [u32]) {
    for (frame, words) in input.iter().zip(output.chunks_exact_mut(2)) {
        let left = (frame[0].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        let right = (frame[1].clamp(-1.0, 1.0) * i32::MAX as f32) as i32 as u32;
        words[0] = left;
        words[1] = right;
    }
}

/// Async task that polls knobs at ~1 kHz and stores values for the audio callback.
///
/// Uses `Ticker` for fixed-period scheduling so ADC read duration does not
/// accumulate drift into the poll interval (TASK-025).
#[embassy_executor::task]
async fn knob_poll_task(knob_state: &'static KnobState) {
    // The Pod's knobs are wired to Seed GPIO breakout pins that `board.pins`
    // also names (d21 == PC4, d15 == PC0). Discard them unread above in
    // main() so this steal() is the only live Peri handle per physical pin.
    let adc1 = unsafe { hal::peripherals::ADC1::steal() };
    let knob1 = unsafe { hal::peripherals::PC4::steal() };
    let knob2 = unsafe { hal::peripherals::PC0::steal() };
    let mut knobs = Knobs::new(adc1, knob1, knob2);

    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_millis(1));

    loop {
        let (k1, k2) = knobs.read();
        knob_state.write([k1, k2]);
        ticker.next().await;
    }
}

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    info!("Booting...");

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals — usb::init() steals them directly via Peri::steal().
    // This field must never be read; using it would create a second Peri handle
    // for the same physical peripheral, defeating Peri's exclusivity guarantee.
    let _ = board.usb_peripherals;

    // Discard knob pins — knob_poll_task steals ADC1/PC4/PC0 directly via
    // Peri::steal(). These fields must not be read; keeping them alive would
    // create duplicate Peri handles for the same physical pins, defeating
    // Peri's exclusivity guarantee. Same invariant as `board.usb_peripherals` above.
    let _ = (
        board.pins.d21, // knob1 / PC4
        board.pins.d15, // knob2 / PC0
    );

    // Init RGB LED — single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Init USB CDC serial logging.
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);
    info!("USB logging initialized");

    // Initialize shared knob state.
    let knob_state = KNOB_STATE.init(KnobState::new());

    // Create processors — OnePoleLowPass (knob 1) → Gain (knob 2).
    let mut filter = OnePoleLowPass::default();
    let mut gain = Gain::default();

    // Set sample rate explicitly to match device actual (48 kHz default).
    filter.set_sample_rate(48_000.0);
    gain.set_sample_rate(48_000.0);

    // Prepare the audio interface (SAI + codec init + DMA buffers)
    let interface = board
        .audio_peripherals
        .prepare_interface(Default::default())
        .await;

    // Start SAI TX/RX and transition to Running state
    let mut interface = match interface.start_interface().await {
        Ok(iface) => iface,
        Err(e) => {
            let _ = e; /* SAI init failed — halt */
            #[allow(clippy::empty_loop)]
            loop {}
        }
    };

    info!("Audio interface ready");

    // Linger on the pre-init red so it can actually be seen. Everything above
    // this point takes a few milliseconds, so without the delay red → green
    // reads as "always green" and the two stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    // Transition LED to Running state (steady green).
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Spawn knob-polling task (~1 kHz interval).
    spawner.spawn(knob_poll_task(knob_state).expect("failed to spawn knob task"));

    // Enter the audio callback loop. Returns Result<Infallible, sai::Error>;
    // Infallible can never be constructed, so this only exits on SAI hardware error.
    // Run USB and LED blink alongside audio using nested selects.
    let usb_fut = asperitas_logging::usb::run();
    let audio_fut = interface.start_callback(|input, output| {
        // Read latest knob positions under critical section.
        let knobs = cortex_m::interrupt::free(|_| knob_state.read());

        // Update processor params at block rate.
        filter.set_params(&OnePoleLowPass::params_from_normalised(knobs[0]));
        gain.set_params(&Gain::params_from_normalised(knobs[1]));

        // Decode input buffer to stack frames.
        let mut frames_in = [Frame::default(); BLOCK_LENGTH];
        decode_block(input, &mut frames_in);

        // Process through DSP chain: low-pass filter → gain.
        let mut frames_out = [Frame::default(); BLOCK_LENGTH];
        filter.process_block(&frames_in, &mut frames_out);

        let mut frames_processed = [Frame::default(); BLOCK_LENGTH];
        gain.process_block(&frames_out, &mut frames_processed);

        // Encode processed frames back to output buffer.
        encode_block(&frames_processed, output);
    });
    let led_fut = asperitas_logging::led::blink_task();

    // Run LED blink alongside the audio+USB pair. The blink_task never returns,
    // so the outer select always yields the inner result (audio or USB ending).
    match embassy_futures::select::select(
        async {
            match embassy_futures::select::select(audio_fut, usb_fut).await {
                embassy_futures::select::Either::First(Ok(_)) => unreachable!(),
                embassy_futures::select::Either::First(Err(e)) => {
                    let _ = e;
                }
                embassy_futures::select::Either::Second(_) => unreachable!(),
            }
        },
        led_fut,
    )
    .await
    {
        embassy_futures::select::Either::First(_) => {}
        embassy_futures::select::Either::Second(_) => unreachable!(),
    }
    #[allow(clippy::empty_loop)]
    loop {}
}
