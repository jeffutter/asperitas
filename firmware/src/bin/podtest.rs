#![no_std]
#![no_main]

use asperitas_logging::info;
use asperitas_pod::encoder::{ControlEvent, ControlSurface};
use asperitas_pod::knob::Knobs;
use asperitas_pod::led::{Led2, Led2Color};
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};

// Re-export the panic handler from asperitas-logging.
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

/// LED 2 colour cycle — Off → Red → Green → Blue → Yellow → Cyan → Magenta → White.
const LED_COLORS: [Led2Color; 8] = [
    Led2Color::Off,
    Led2Color::Red,
    Led2Color::Green,
    Led2Color::Blue,
    Led2Color::Yellow,
    Led2Color::Cyan,
    Led2Color::Magenta,
    Led2Color::White,
];

/// Poll interval for control surface (~1 kHz).
///
/// `ControlSurface::poll()` requires ~1 kHz polling so that its Gray-code
/// quadrature LUT sees at most one A/B transition per call. At lower rates,
/// brisk encoder turns can produce two transitions within one window, causing
/// silent detent drops (see `crates/asperitas-pod/src/encoder.rs`).
///
/// Used with `embassy_time::Ticker::every()` for fixed-period scheduling;
/// work duration does not accumulate drift into the interval (TASK-025).
const POLL_INTERVAL_MS: u64 = 1;

/// Throttle factor for knob-value logging over USB CDC.
///
/// Knob values are logged every `KNOB_LOG_THROTTLE` ticks, keeping serial
/// output at ~100 Hz while the main loop runs at ~1 kHz.
const KNOB_LOG_THROTTLE: u32 = 10;

/// Number of polls between LED colour changes (~1 second at 1 kHz).
const LED_TICKS_PER_COLOR: u32 = 1000;

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    info!("[podtest] booting");

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals — usb::init() steals them directly via Peri::steal().
    let _ = board.usb_peripherals;

    // Init LED 1 (boot/panic indicator) — single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Init USB CDC serial logging.
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);

    // Linger on the pre-init red so it can actually be seen. Boot takes a few
    // milliseconds, so without the delay red → green reads as "always green"
    // and the two stages can't be distinguished by eye — which is exactly what
    // TASK-018.04 AC #6 asks a human to confirm. Mirrors main.rs.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    // Transition LED 1 to Running state (steady green).
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // The Pod's control-surface pins are wired to Seed GPIO breakout pins that
    // `board.pins` also names (e.g. d26 == PD11, the encoder A pin below).
    // Discard them here, unread, so the steal() calls that follow are the only
    // live Peri handle to each physical pin — same invariant documented for
    // `board.usb_peripherals` above and for asperitas_logging::usb::init's own
    // steal() (crates/asperitas-logging/src/usb.rs).
    let _ = (
        board.pins.d21, // knob1 / PC4
        board.pins.d15, // knob2 / PC0
        board.pins.d26, // enc_a / PD11
        board.pins.d25, // enc_b / PA0
        board.pins.d13, // click / PB6
        board.pins.d27, // button1 / PG9
        board.pins.d28, // button2 / PA2
        board.pins.d17, // led2 red / PB1
        board.pins.d24, // led2 green / PA1
        board.pins.d23, // led2 blue / PA4
    );

    // Steal Pod hardware peripherals. Safe on single-core Cortex-M because we
    // control all access and the corresponding board.pins fields were just
    // discarded unread above.
    let adc1 = unsafe { hal::peripherals::ADC1::steal() };
    let knob1_pin = unsafe { hal::peripherals::PC4::steal() };
    let knob2_pin = unsafe { hal::peripherals::PC0::steal() };
    let mut knobs = Knobs::new(adc1, knob1_pin, knob2_pin);

    let enc_a = unsafe { hal::peripherals::PD11::steal() };
    let enc_b = unsafe { hal::peripherals::PA0::steal() };
    let click = unsafe { hal::peripherals::PB6::steal() };
    let button1 = unsafe { hal::peripherals::PG9::steal() };
    let button2 = unsafe { hal::peripherals::PA2::steal() };
    let mut controls = ControlSurface::new(enc_a, enc_b, click, button1, button2);

    let red_pin = unsafe { hal::peripherals::PB1::steal() };
    let green_pin = unsafe { hal::peripherals::PA1::steal() };
    let blue_pin = unsafe { hal::peripherals::PA4::steal() };
    let mut led2 = Led2::new(red_pin, green_pin, blue_pin);

    info!("[podtest] running");

    // Main polling loop — ~1 kHz (ControlSurface contract).
    // Uses Ticker for fixed-period scheduling so work duration does not
    // accumulate drift into the poll interval (see TASK-025).
    let mut ticker = embassy_time::Ticker::every(
        embassy_time::Duration::from_millis(POLL_INTERVAL_MS)
    );
    let mut tick: u32 = 0;
    let mut knob_log_tick: u32 = 0;
    let mut color_idx: usize = 0;

    // Set initial LED colour.
    led2.set_color(LED_COLORS[color_idx]);
    info!(
        "[podtest] t={} led2={}",
        embassy_time::Instant::now().as_millis(),
        color_name(LED_COLORS[color_idx])
    );

    // Bound catch-up bursts after executor stalls (TASK-026).
    let mut last_tick = embassy_time::Instant::now();

    let poll_fut = async {
        loop {
            // Milliseconds since boot, stamped on every line. Without it the
            // log has no timebase: segments of a hand-run test cannot be told
            // apart, hold durations cannot be measured, and the sample rate
            // has to be inferred. Cheap, and it makes the capture analysable.
            let now_ms = embassy_time::Instant::now().as_millis();

            // Poll knobs — throttled via KNOB_LOG_THROTTLE to keep USB CDC
            // serial readable.
            //
            // Logged as RAW ADC counts, not normalised permille. Jitter is the
            // measurement this harness exists for (TASK-018.04 AC #2), and
            // permille quantises to 1/1000 — coarse enough to floor the very
            // number being measured. Full scale is 65535; see
            // asperitas_pod::knob::POT_FULL_SCALE_COUNTS.
            let (r1, r2) = knobs.read_raw();
            if knob_log_tick.is_multiple_of(KNOB_LOG_THROTTLE) {
                info!("[podtest] t={} r1={} r2={}", now_ms, r1, r2);
            }

            // Poll encoder and buttons.
            controls.poll(|event| match event {
                ControlEvent::EncoderDelta(delta) => {
                    info!("[podtest] t={} ENC {:+}", now_ms, delta);
                }
                ControlEvent::ClickPress => {
                    info!("[podtest] t={} CLICK press", now_ms);
                }
                ControlEvent::ClickRelease => {
                    info!("[podtest] t={} CLICK release", now_ms);
                }
                ControlEvent::Button1Press => {
                    info!("[podtest] t={} BTN1 press", now_ms);
                }
                ControlEvent::Button1Release => {
                    info!("[podtest] t={} BTN1 release", now_ms);
                }
                ControlEvent::Button2Press => {
                    info!("[podtest] t={} BTN2 press", now_ms);
                }
                ControlEvent::Button2Release => {
                    info!("[podtest] t={} BTN2 release", now_ms);
                }
            });

            // Advance counters.
            tick += 1;
            knob_log_tick = knob_log_tick.wrapping_add(1);

            // Cycle LED 2 every LED_TICKS_PER_COLOR ticks (~1 second).
            if tick >= LED_TICKS_PER_COLOR {
                tick = 0;
                color_idx = (color_idx + 1) % LED_COLORS.len();
                led2.set_color(LED_COLORS[color_idx]);
                info!(
                    "[podtest] t={} led2={}",
                    embassy_time::Instant::now().as_millis(),
                    color_name(LED_COLORS[color_idx])
                );
            }

            // Bound catch-up bursts: if the executor stalled longer than twice
            // the poll period, reset the ticker to discard its backlog instead
            // of letting next() fire all missed ticks back-to-back.
            let now = embassy_time::Instant::now();
            if now - last_tick > embassy_time::Duration::from_millis(POLL_INTERVAL_MS * 2) {
                ticker.reset();
            } else {
                ticker.next().await;
            }
            last_tick = now;
        }
    };

    // Run USB drain alongside the polling loop.
    let usb_fut = asperitas_logging::usb::run();
    let _ = embassy_futures::select::select(poll_fut, usb_fut).await;
}

/// Return a lowercase name string for an LED colour.
fn color_name(color: Led2Color) -> &'static str {
    match color {
        Led2Color::Off => "off",
        Led2Color::Red => "red",
        Led2Color::Green => "green",
        Led2Color::Blue => "blue",
        Led2Color::Yellow => "yellow",
        Led2Color::Cyan => "cyan",
        Led2Color::Magenta => "magenta",
        Led2Color::White => "white",
    }
}
