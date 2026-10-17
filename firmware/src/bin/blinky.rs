#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use asperitas_logging::info;
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::DaisyBoard;

use asperitas_firmware as _;

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
    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board: DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);

    // Discard USB peripherals — `asperitas_logging::init` steals them directly via T::steal().
    // This field must never be read; using it would create a second Peri handle
    // for the same physical peripheral, defeating Peri's exclusivity guarantee.
    let _ = board.usb_peripherals;

    // Init RGB LED — single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Bring up logging. Records logged before this call are discarded.
    let console = asperitas_logging::init(UsbIrqs);
    info!("Blinky booting...");
    info!("Blinky running");

    // Transition LED to Running state (steady green) after boot completes.
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Run the console and LED blink concurrently.
    let console_fut = console.run();
    let led_fut = asperitas_logging::led::blink_task();

    // Neither future completes; select polls both forever.
    let _ = embassy_futures::select::select(led_fut, console_fut).await;
}
