#![no_std]
#![no_main]

use asperitas_logging::info;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::DaisyBoard;

// Re-export the panic handler from asperitas-logging.
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
// nobody to scan RAM, so the stub discards every byte — silent, but still required.
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

bind_interrupts!(pub struct UsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

/// Blinky — known-good diagnostic for Seed3.
///
/// RGB LED (PC1/PA6/PA7) shows PreInit (steady red) during boot, then
/// transitions to Running (steady green) once USB is up. Boot takes only a few
/// milliseconds, so the red stage is a flicker in practice — `blink_task` would
/// blink it at ~1 Hz, but it isn't running yet while the state is still PreInit.
/// On panic, the LED turns steady red.
/// Flash via DFU; see `docs/reference/daisy-seed3.md` or run `make flash-all`.
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Install the backend before the first record exists; with the console compiled in,
    // `usb::init` below does it instead.
    #[cfg(not(feature = "log-usb"))]
    asperitas_logging::init();

    info!("Blinky booting...");

    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board: DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);

    // Discard USB peripherals — usb::init() steals them directly via T::steal().
    // This field must never be read; using it would create a second Peri handle
    // for the same physical peripheral, defeating Peri's exclusivity guarantee.
    let _ = board.usb_peripherals;

    // Init RGB LED — single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Init USB CDC serial logging.
    #[cfg(feature = "log-usb")]
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);
    info!("Blinky running");

    // Transition LED to Running state (steady green) after boot completes.
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Run the console and LED blink concurrently.
    #[cfg(feature = "log-usb")]
    let console_fut = asperitas_logging::usb::run();
    // No drain task without the console; RTT is read out of RAM by the probe. See main.rs.
    #[cfg(not(feature = "log-usb"))]
    let console_fut = core::future::pending::<()>();
    let led_fut = asperitas_logging::led::blink_task();

    // Neither future completes; select polls both forever.
    let _ = embassy_futures::select::select(led_fut, console_fut).await;
}
