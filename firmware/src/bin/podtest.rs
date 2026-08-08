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

/// Poll interval for control surface (~100 Hz).
const POLL_INTERVAL_MS: u64 = 10;

/// Number of polls between LED colour changes (~1 second at 100 Hz).
const LED_TICKS_PER_COLOR: u32 = 100;

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

    // Transition LED 1 to Running state (steady green).
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Steal Pod hardware peripherals — daisy-embassy does not expose Pod pins,
    // so steal() is safe on single-core Cortex-M where we control all access.
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

    // Main polling loop — ~100 Hz.
    let mut tick: u32 = 0;
    let mut color_idx: usize = 0;

    // Set initial LED colour.
    led2.set_color(LED_COLORS[color_idx]);
    info!("[podtest] led2={}", color_name(LED_COLORS[color_idx]));

    let poll_fut = async {
        loop {
            // Poll knobs — quantise to permille (×1000) to avoid float fmt on no_std.
            let (k1, k2) = knobs.read();
            let k1_i = (k1 * 1000.0) as u16;
            let k2_i = (k2 * 1000.0) as u16;
            info!("[podtest] k1={} k2={}", k1_i, k2_i);

            // Poll encoder and buttons.
            controls.poll(|event| match event {
                ControlEvent::EncoderDelta(delta) => {
                    info!("[podtest] ENC {:+}", delta);
                }
                ControlEvent::ClickPress => {
                    info!("[podtest] CLICK press");
                }
                ControlEvent::ClickRelease => {
                    info!("[podtest] CLICK release");
                }
                ControlEvent::Button1Press => {
                    info!("[podtest] BTN1 press");
                }
                ControlEvent::Button1Release => {
                    info!("[podtest] BTN1 release");
                }
                ControlEvent::Button2Press => {
                    info!("[podtest] BTN2 press");
                }
                ControlEvent::Button2Release => {
                    info!("[podtest] BTN2 release");
                }
            });

            // Cycle LED 2 every LED_TICKS_PER_COLOR ticks (~1 second).
            tick += 1;
            if tick >= LED_TICKS_PER_COLOR {
                tick = 0;
                color_idx = (color_idx + 1) % LED_COLORS.len();
                led2.set_color(LED_COLORS[color_idx]);
                info!("[podtest] led2={}", color_name(LED_COLORS[color_idx]));
            }

            embassy_time::Timer::after_millis(POLL_INTERVAL_MS).await;
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
