#![no_std]
#![no_main]

use daisy_embassy::{DaisyBoard, hal, new_daisy_board};
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use asperitas_logging::info;

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

/// Audio passthrough — input copied directly to output.
///
/// 48 kHz, 32-sample blocks (daisy-embassy default). TAC5242 codec,
/// hardware-strapped on Seed3 so no I²C init needed.
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    info!("Booting...");

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Init RGB LED — single owner for boot stages + panic handler.
    asperitas_logging::led::init(
        board.pins.d20,
        board.pins.d19,
        board.pins.d18,
    );

    // Init USB CDC serial logging.
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);
    info!("USB logging initialized");

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

    // Transition LED to Running state (steady green).
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Enter the audio callback loop. Returns Result<Infallible, sai::Error>;
    // Infallible can never be constructed, so this only exits on SAI hardware error.
    // Run USB and LED blink alongside audio using nested selects.
    let usb_fut = asperitas_logging::usb::run();
    let audio_fut = interface.start_callback(|input, output| {
        output.copy_from_slice(input);
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
