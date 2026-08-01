#![no_std]
#![no_main]

use embassy_time::Timer;
use daisy_embassy::DaisyBoard;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use asperitas_logging::info;

// Re-export the panic handler from asperitas-logging.
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    asperitas_logging::panic_handler::handle_panic(info)
}

// Provide _defmt_panic symbol required by embassy-stm32 / embassy-usb's
// internal defmt usage (defmt 0.3.x). This is NOT the Rust panic handler;
// it fires only when a defmt formatter encounters an unrecoverable error.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    cortex_m::asm::bkpt();
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

/// Blinky — known-good diagnostic for Seed3.
///
/// Toggles the onboard user LED (PC7) at ~1.6 Hz (300 ms on, 300 ms off).
/// Flash via DFU; see `docs/reference/daisy-seed3.md` or run `make flash-all`.
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    info!("Blinky booting...");

    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board: DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);

    // Install panic LED (consumes RGB pins PC1/PA6/PA7).
    asperitas_logging::panic_handler::set_panic_led(
        board.pins.d20,
        board.pins.d19,
        board.pins.d18,
    );

    // Init USB CDC serial logging.
    let _usb_handle = asperitas_logging::usb::init(
        board.usb_peripherals.usb_otg_fs,
        board.usb_peripherals.pins.DP,
        board.usb_peripherals.pins.DN,
        UsbIrqs,
    );
    info!("Blinky running");

    let mut led = board.user_led;

    // Run USB alongside the blink loop.
    let usb_fut = asperitas_logging::usb::run();
    let blink = async {
        loop {
            led.on();
            Timer::after_millis(300).await;
            led.off();
            Timer::after_millis(300).await;
        }
    };

    // Neither future completes; select polls both forever.
    let _ = embassy_futures::select::select(blink, usb_fut).await;
}
