#![no_std]
#![no_main]

use embassy_time::Timer;
use defmt;
use panic_halt as _;

// No-op defmt logger — satisfies linker symbols required by embassy-stm32's
// internal defmt usage. Remove when adding real logging (e.g. defmt-rtt).
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

#[no_mangle]
unsafe extern "C" fn _defmt_panic() -> ! {
    loop {}
}

/// Blinky — known-good diagnostic for Seed3.
///
/// Toggles the onboard user LED (PC7) at ~1.6 Hz (300 ms on, 300 ms off).
/// Flash via DFU; see `docs/reference/daisy-seed3.md` or run `make flash-all`.
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board: daisy_embassy::DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);
    let mut led = board.user_led;

    loop {
        led.on();
        Timer::after_millis(300).await;
        led.off();
        Timer::after_millis(300).await;
    }
}
