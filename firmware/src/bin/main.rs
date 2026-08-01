#![no_std]
#![no_main]

use daisy_embassy::{DaisyBoard, hal, new_daisy_board};
use panic_halt as _;

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

// defmt panic handler — loops forever (same effect as panic-halt)
#[no_mangle]
#[allow(clippy::empty_loop)]
unsafe extern "C" fn _defmt_panic() -> ! {
    loop {}
}

/// Audio passthrough — input copied directly to output.
///
/// 48 kHz, 32-sample blocks (daisy-embassy default). TAC5242 codec,
/// hardware-strapped on Seed3 so no I²C init needed.
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

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

    // Enter the audio callback loop. Returns Result<Infallible, sai::Error>;
    // Infallible can never be constructed, so this only exits on SAI hardware error.
    #[allow(irrefutable_let_patterns)]
    let Err(e) = interface.start_callback(|input, output| {
        output.copy_from_slice(input);
    }).await;
    let _ = e;
    #[allow(clippy::empty_loop)]
    loop {}
}
