#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use core::sync::atomic::{AtomicU32, Ordering};

use asperitas_firmware::audio::{self, decode_block, encode_block, AudioProcessor, BLOCK_LENGTH};
use asperitas_firmware::host_control;

#[cfg(not(feature = "resonator"))]
use asperitas_dsp::asperitas::{Asperitas, Controls};
use asperitas_dsp::processor::{Frame, Processor};
#[cfg(feature = "resonator")]
use asperitas_dsp::{Resonator, ResonatorControls};
use asperitas_logging::info;
#[cfg(feature = "resonator")]
use asperitas_pod::encoder::ControlSurface;
use asperitas_pod::knob::Knobs;
#[cfg(feature = "resonator")]
use asperitas_pod::led::Led2;
#[cfg(feature = "resonator")]
use asperitas_pod::resonator_surface::{led_level, led_root_colour, ResonatorSurface, Rgb};
use asperitas_pod::ticker_guard::should_reset;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use static_cell::StaticCell;

use asperitas_firmware as _;

bind_interrupts!(pub struct UsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

/// Shared control values - written by the control-surface task, read by the audio callback.
///
/// Each value is an `f32` stored as its bit pattern in an `AtomicU32`, so the audio interrupt
/// can preempt the poll task at any point and still read whole values. The values are
/// independent atomics: a read may pair a new value with the previous one of its neighbour,
/// which the per-block smoothing in the effect absorbs.
struct ControlState<const N: usize> {
    values: [AtomicU32; N],
}

impl<const N: usize> ControlState<N> {
    const fn new() -> Self {
        Self {
            values: [const { AtomicU32::new(0) }; N],
        }
    }

    /// Write new values from the polling task.
    fn write(&self, values: [f32; N]) {
        for (slot, v) in self.values.iter().zip(values) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// Read current values from the audio callback.
    fn read(&self) -> [f32; N] {
        core::array::from_fn(|i| f32::from_bits(self.values[i].load(Ordering::Relaxed)))
    }
}

/// The patch's two knobs.
#[cfg(not(feature = "resonator"))]
static KNOB_STATE: ControlState<2> = ControlState::new();

/// The resonator's five control positions: root, structure, damping, spread, preset.
#[cfg(feature = "resonator")]
static CONTROL_STATE: ControlState<{ asperitas_dsp::RESONATOR_KNOB_COUNT }> = ControlState::new();

/// Peak of the last audio block's input, linear, for LED 2. A single value the callback
/// overwrites each block and the control task samples.
#[cfg(feature = "resonator")]
static INPUT_PEAK: AtomicU32 = AtomicU32::new(0);

/// Poll interval for control surface (~1 kHz).
const POLL_INTERVAL_MS: u64 = 1;

/// The audio callback's state: the shared knob readings and the effect they drive.
#[cfg(not(feature = "resonator"))]
struct KnobChain {
    knob_state: &'static ControlState<2>,
    effect: Asperitas,
}

#[cfg(not(feature = "resonator"))]
impl AudioProcessor for KnobChain {
    fn process(&mut self, input: &[u32], output: &mut [u32]) {
        let knobs = host_control::merge(self.knob_state.read());

        // Controls are applied once per block; the effect smooths the change.
        self.effect.set_controls(&Controls { knobs });

        let mut frames = [Frame::default(); BLOCK_LENGTH];
        decode_block(input, &mut frames);
        self.effect.process_block(&mut frames);
        encode_block(&frames, output);
    }
}

/// The audio callback's state when `main` runs the resonator: the shared control positions and
/// the effect they drive.
///
/// Per block the callback reads five atomics, merges host overrides (one critical section around
/// a five-float copy), latches the parameters, processes, and stores the input's peak for LED 2.
/// It allocates nothing and holds no lock across the block.
#[cfg(feature = "resonator")]
struct ResonatorChain {
    state: &'static ControlState<{ asperitas_dsp::RESONATOR_KNOB_COUNT }>,
    effect: Resonator,
}

#[cfg(feature = "resonator")]
impl AudioProcessor for ResonatorChain {
    fn process(&mut self, input: &[u32], output: &mut [u32]) {
        let positions = host_control::merge(self.state.read());

        // Controls are applied once per block; the effect smooths the change.
        self.effect
            .set_params(&ResonatorControls { positions }.params());

        let mut frames = [Frame::default(); BLOCK_LENGTH];
        decode_block(input, &mut frames);
        let peak = frames
            .iter()
            .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
        INPUT_PEAK.store(peak.to_bits(), Ordering::Relaxed);
        self.effect.process_block(&mut frames);
        encode_block(&frames, output);
    }
}

asperitas_firmware::audio_sai1_vector!();

#[cfg(not(feature = "resonator"))]
static CHAIN: StaticCell<KnobChain> = StaticCell::new();
#[cfg(feature = "resonator")]
static CHAIN: StaticCell<ResonatorChain> = StaticCell::new();

/// Async task that polls knobs at ~1 kHz and stores values for the audio callback.
///
/// Uses `Ticker` for fixed-period scheduling so ADC read duration does not
/// accumulate drift into the poll interval.
/// Bounded catch-up: if the executor stalls, the ticker is reset
/// on overrun rather than replaying missed ticks back-to-back.
#[cfg(not(feature = "resonator"))]
#[embassy_executor::task]
async fn knob_poll_task(knob_state: &'static ControlState<2>, mut knobs: Knobs) {
    let mut ticker =
        embassy_time::Ticker::every(embassy_time::Duration::from_millis(POLL_INTERVAL_MS));
    let mut last_tick = embassy_time::Instant::now();

    loop {
        let (k1, k2) = knobs.read();
        knob_state.write([k1, k2]);

        // Bound catch-up bursts: if the executor stalled longer than twice the
        // poll period, reset the ticker to discard its backlog instead of
        // letting next() fire all missed ticks back-to-back. Always await the
        // (possibly just-reset) ticker afterward so the loop cannot spin
        // through an extra zero-wait iteration.
        let now = embassy_time::Instant::now();
        if should_reset(
            now,
            last_tick,
            embassy_time::Duration::from_millis(POLL_INTERVAL_MS),
        ) {
            ticker.reset();
        }
        ticker.next().await;
        last_tick = now;
    }
}

/// How often LED 2 and the fine-tune blink are refreshed, in poll ticks (50 ms).
#[cfg(feature = "resonator")]
const LED_TICKS: u32 = 50;

/// Fine-tune blinks LED 1 at about 2 Hz: the colour toggles every this many LED refreshes.
#[cfg(feature = "resonator")]
const FINE_BLINK_REFRESHES: u32 = 5;

/// Control task for the resonator build: polls the knobs, encoder and buttons at ~1 kHz, applies
/// the events to the Pod mapping, publishes the five control positions for the audio callback, and
/// drives both LEDs (LED 1 root colour, LED 2 input level).
///
/// Same ticker and catch-up bound as `knob_poll_task`; `ControlSurface::poll` needs the 1 kHz
/// cadence so it sees at most one quadrature transition per call.
#[cfg(feature = "resonator")]
#[embassy_executor::task]
async fn resonator_control_task(
    state: &'static ControlState<{ asperitas_dsp::RESONATOR_KNOB_COUNT }>,
    mut knobs: Knobs,
    mut controls: ControlSurface,
    mut led2: Led2,
) {
    let mut surface = ResonatorSurface::default();
    let mut ticker =
        embassy_time::Ticker::every(embassy_time::Duration::from_millis(POLL_INTERVAL_MS));
    let mut last_tick = embassy_time::Instant::now();
    let mut tick: u32 = 0;
    let mut refreshes: u32 = 0;
    let mut peak = 0.0f32;
    let mut last_led2: Option<Rgb> = None;
    let mut last_led1: Option<Rgb> = None;

    loop {
        let (k1, k2) = knobs.read();
        controls.poll(|event| surface.apply_event(event));
        state.write(surface.positions([k1, k2]));

        // The callback overwrites the peak every 0.67 ms; keep the largest seen over the window.
        let sample = f32::from_bits(INPUT_PEAK.load(Ordering::Relaxed));
        if sample > peak {
            peak = sample;
        }
        tick += 1;
        if tick >= LED_TICKS {
            tick = 0;
            refreshes = refreshes.wrapping_add(1);

            let level = led_level(peak);
            peak = 0.0;
            if last_led2 != Some(level) {
                led2.set_channels(level.r, level.g, level.b);
                last_led2 = Some(level);
            }

            let root = led_root_colour(surface.root_hz());
            let lit = !surface.fine() || (refreshes / FINE_BLINK_REFRESHES).is_multiple_of(2);
            let shown = if lit {
                root
            } else {
                asperitas_pod::resonator_surface::RGB_OFF
            };
            if last_led1 != Some(shown) {
                asperitas_logging::led::set_user_colour(shown.r, shown.g, shown.b);
                last_led1 = Some(shown);
            }
        }

        let now = embassy_time::Instant::now();
        if should_reset(
            now,
            last_tick,
            embassy_time::Duration::from_millis(POLL_INTERVAL_MS),
        ) {
            ticker.reset();
        }
        ticker.next().await;
        last_tick = now;
    }
}

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    // Before anything else can touch RCC_RSR: CTL RSN reports what this read.
    host_control::capture_reset_reason();

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals — `asperitas_logging::init` steals them directly via Peri::steal().
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
    // The resonator build also owns the encoder, its click, both buttons and LED 2, stolen below
    // under the same rule.
    #[cfg(feature = "resonator")]
    let _ = (
        board.pins.d26, // enc_a / PD11
        board.pins.d25, // enc_b / PA0
        board.pins.d13, // click / PB6
        board.pins.d27, // button1 / PG9
        board.pins.d28, // button2 / PA2
        board.pins.d17, // led2 red / PB1
        board.pins.d24, // led2 green / PA1
        board.pins.d23, // led2 blue / PA4
    );

    // Init RGB LED — single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Bring up logging. Records logged before this call are discarded.
    let console = asperitas_logging::init(UsbIrqs);
    info!("Booting...");
    // Host control (VER/RSN/RST/SET/REL). Must precede `console.run()` below, which reads the
    // inbound enable flag once when it starts.
    host_control::spawn(&spawner);

    // Seed the shared knob state with the effect's defaults so the audio callback never sees
    // zeros before the first poll.
    #[cfg(not(feature = "resonator"))]
    let knob_state = &KNOB_STATE;
    #[cfg(not(feature = "resonator"))]
    knob_state.write(Controls::default().knobs);
    #[cfg(feature = "resonator")]
    let control_state = &CONTROL_STATE;
    #[cfg(feature = "resonator")]
    control_state.write(ResonatorSurface::default().positions([
        ResonatorControls::default().positions[1],
        ResonatorControls::default().positions[2],
    ]));

    // Bring the knob ADC up here, before the audio interface starts. `Knobs::new` enables and
    // calibrates the ADC with blocking waits of a millisecond or more; run inside the knob task
    // it stalls the executor right after the SAI starts, longer than the DMA ring holds, and the
    // first `read` reports `Overrun` (AUDIO_EXIT == 3), which ends `main` before a single block
    // is processed.
    //
    // The Pod's knobs are wired to Seed GPIO breakout pins that `board.pins` also names
    // (d21 == PC4, d15 == PC0). The tuple above discards them unread, so these steals are the
    // only live Peri handles per physical pin.
    let knobs = {
        let adc1 = unsafe { hal::peripherals::ADC1::steal() };
        let knob1 = unsafe { hal::peripherals::PC4::steal() };
        let knob2 = unsafe { hal::peripherals::PC0::steal() };
        Knobs::new(adc1, knob1, knob2)
    };

    // The rest of the Pod's control surface, for the resonator build. The matching `board.pins`
    // fields were discarded above, so these are the only live handles.
    #[cfg(feature = "resonator")]
    let (surface, led2) = {
        let enc_a = unsafe { hal::peripherals::PD11::steal() };
        let enc_b = unsafe { hal::peripherals::PA0::steal() };
        let click = unsafe { hal::peripherals::PB6::steal() };
        let button1 = unsafe { hal::peripherals::PG9::steal() };
        let button2 = unsafe { hal::peripherals::PA2::steal() };
        let red = unsafe { hal::peripherals::PB1::steal() };
        let green = unsafe { hal::peripherals::PA1::steal() };
        let blue = unsafe { hal::peripherals::PA4::steal() };
        (
            ControlSurface::new(enc_a, enc_b, click, button1, button2),
            Led2::new(red, green, blue),
        )
    };

    // The effect runs at the device's 48 kHz sample rate.
    #[cfg(not(feature = "resonator"))]
    let chain = CHAIN.init(KnobChain {
        knob_state,
        effect: Asperitas::new(48_000.0, &Controls::default()),
    });
    #[cfg(feature = "resonator")]
    let chain = CHAIN.init(ResonatorChain {
        state: control_state,
        effect: Resonator::new(48_000.0, &ResonatorControls::default().params()),
    });

    // Prepare the audio interface (SAI + codec init + DMA buffers)
    let interface = board
        .audio_peripherals
        .prepare_interface(Default::default())
        .await;

    // Linger on the pre-init red so it can actually be seen. Everything above
    // this point takes a few milliseconds, so without the delay red -> green
    // reads as "always green" and the two stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    // Transition LED to Running state (steady green). `audio::start` moves it to Panicked if the
    // interface then fails to start.
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Spawn the control-polling task (~1 kHz interval).
    #[cfg(not(feature = "resonator"))]
    spawner.spawn(knob_poll_task(knob_state, knobs).expect("failed to spawn knob task"));
    #[cfg(feature = "resonator")]
    spawner.spawn(
        resonator_control_task(control_state, knobs, surface, led2)
            .expect("failed to spawn control task"),
    );

    // Start the interface and the audio callback on its own interrupt executor.
    audio::start(interface, chain).await;

    // Thread mode keeps what has no deadline: the console and the LED blink.
    let console_fut = console.run();
    let led_fut = asperitas_logging::led::blink_task();

    // Neither returns. If one does, the audio interrupt would keep playing with nothing
    // reporting on it, so halt instead.
    embassy_futures::select::select(console_fut, led_fut).await;
    #[allow(clippy::empty_loop)]
    loop {}
}
