#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use core::sync::atomic::{AtomicU32, Ordering};

use asperitas_firmware::audio::{self, decode_block, encode_block, AudioProcessor, BLOCK_LENGTH};
use asperitas_firmware::host_control;

use asperitas_dsp::asperitas::{Asperitas, Controls};
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_logging::info;
use asperitas_pod::knob::Knobs;
use asperitas_pod::ticker_guard::should_reset;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use static_cell::StaticCell;

use asperitas_firmware as _;

bind_interrupts!(pub struct UsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

/// Shared knob readings - written by the control-surface task, read by the audio callback.
///
/// Each knob is an `f32` stored as its bit pattern in an `AtomicU32`, so the audio interrupt
/// can preempt the poll task at any point and still read whole values. The two knobs are
/// independent atomics: a read may pair a new knob 1 with the previous knob 2, which the
/// per-block smoothing in the effect absorbs.
struct KnobState {
    knobs: [AtomicU32; 2],
}

impl KnobState {
    const fn new() -> Self {
        Self {
            knobs: [AtomicU32::new(0), AtomicU32::new(0)],
        }
    }

    /// Write new knob values from the polling task.
    fn write(&self, values: [f32; 2]) {
        for (slot, v) in self.knobs.iter().zip(values) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// Read current knob values from the audio callback.
    fn read(&self) -> [f32; 2] {
        [
            f32::from_bits(self.knobs[0].load(Ordering::Relaxed)),
            f32::from_bits(self.knobs[1].load(Ordering::Relaxed)),
        ]
    }
}

static KNOB_STATE: KnobState = KnobState::new();

/// Poll interval for control surface (~1 kHz).
const POLL_INTERVAL_MS: u64 = 1;

/// The audio callback's state: the shared knob readings and the effect they drive.
struct KnobChain {
    knob_state: &'static KnobState,
    effect: Asperitas,
}

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

asperitas_firmware::audio_sai1_vector!();

static CHAIN: StaticCell<KnobChain> = StaticCell::new();

/// Async task that polls knobs at ~1 kHz and stores values for the audio callback.
///
/// Uses `Ticker` for fixed-period scheduling so ADC read duration does not
/// accumulate drift into the poll interval (TASK-025).
/// Bounded catch-up (TASK-026): if the executor stalls, the ticker is reset
/// on overrun rather than replaying missed ticks back-to-back.
#[embassy_executor::task]
async fn knob_poll_task(knob_state: &'static KnobState, mut knobs: Knobs) {
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

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    // Before anything else can touch RCC_RSR: CTL RSN reports what this read.
    host_control::capture_reset_reason();

    // Install the backend before the first record exists. Under `log-usb` this would be wrong —
    // the logger cannot be installed until the device is live, and [`usb::init`] below does it —
    // so it is only this configuration's entry point. Everything logged before the console comes
    // up is invisible on USB; over RTT it is not, which is why boot records are worth checking
    // here rather than at "USB logging initialized".
    #[cfg(not(feature = "log-usb"))]
    asperitas_logging::init();

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
    #[cfg(feature = "log-usb")]
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);
    #[cfg(feature = "log-usb")]
    info!("USB logging initialized");
    // Host control (VER/RSN/RST/SET/REL). Must precede `usb::run()` below, which reads the
    // enable flag once when it starts.
    #[cfg(feature = "log-usb")]
    host_control::spawn(&spawner);

    // Seed the shared knob state with the effect's defaults so the audio callback never sees
    // zeros before the first poll.
    let knob_state = &KNOB_STATE;
    knob_state.write(Controls::default().knobs);

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

    // The effect runs at the device's 48 kHz sample rate.
    let chain = CHAIN.init(KnobChain {
        knob_state,
        effect: Asperitas::new(48_000.0, &Controls::default()),
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

    // Spawn knob-polling task (~1 kHz interval).
    spawner.spawn(knob_poll_task(knob_state, knobs).expect("failed to spawn knob task"));

    // Start the interface and the audio callback on its own interrupt executor.
    audio::start(interface, chain).await;

    // Thread mode keeps what has no deadline: the console and the LED blink.
    #[cfg(feature = "log-usb")]
    let console_fut = asperitas_logging::usb::run();
    // No drain task exists without the console, and RTT needs no task to exist at all — the
    // probe reads RAM behind our back. `pending()` stands in so the select below is written
    // once instead of twice: it never wakes, exactly like the drain task it replaces.
    #[cfg(not(feature = "log-usb"))]
    let console_fut = core::future::pending::<()>();
    let led_fut = asperitas_logging::led::blink_task();

    // Neither returns. If one does, the audio interrupt would keep playing with nothing
    // reporting on it, so halt instead.
    embassy_futures::select::select(console_fut, led_fut).await;
    #[allow(clippy::empty_loop)]
    loop {}
}
