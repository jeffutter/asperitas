#![no_std]
#![no_main]

use daisy_embassy::DaisyBoard;
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

// defmt panic handler — loops forever (same effect as panic-halt)
#[no_mangle]
unsafe extern "C" fn _defmt_panic() -> ! {
    loop {}
}

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let _board: DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);

    loop {}
}
