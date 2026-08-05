//! Panic-path bring-up — proves a panic actually reaches the developer.
//!
//! The probe-free debug channel has two halves: the Pod LED and USB CDC serial.
//! Ordinary firmware only exercises them on the happy path, so a panic path that
//! silently swallows its message looks identical to a panic that never happened.
//! This binary panics on purpose, at a known line, with a known message, so both
//! halves can be checked against a known-good expectation.
//!
//! Unlike `ledtest.rs` — which deliberately avoids `asperitas_logging` in order to
//! rule it out — this binary uses the shared panic handler on purpose. That
//! handler *is* the thing under test.
//!
//! # Running it
//!
//! ```text
//! make flash-all BINARY=panictest
//! screen /dev/cu.usbmodem<N> 115200     # attach within the countdown
//! ```
//!
//! The board enumerates as "Asperitas Debug Console" and then counts down for
//! [`PANIC_DELAY_SECS`] seconds before panicking, which is the window in which to
//! get a terminal attached. If you miss it, tap RESET — the countdown restarts.
//!
//! # Reading the output
//!
//! | Stage     | LED           | Serial                               |
//! |-----------|---------------|--------------------------------------|
//! | Boot      | steady red    | —                                    |
//! | Countdown | steady green  | `panictest: panicking in N...` ×N    |
//! | Panicked  | steady red    | `PANIC: <msg> at src/bin/panictest.rs:L:C` |
//!
//! All three stages must appear. Specifically:
//!
//! - The countdown lines prove the ordinary pipe → drain-loop → endpoint path
//!   works; if the LED counts down but no text arrives, the fault is there.
//! - The `PANIC:` line proves the *panic* path works, which is a different
//!   mechanism: the executor is dead by then, so that line is pushed to the
//!   endpoint synchronously by `usb::emit_blocking`. Countdown text without a
//!   `PANIC:` line is precisely the failure this binary exists to catch.
//! - The line must end cleanly at the source location, with no trailing NUL
//!   garbage.
//! - Green → red is the LED half of the same signal, and is the only half that
//!   works with no host attached.

#![no_std]
#![no_main]

use asperitas_logging::info;
use asperitas_logging::led::{self, LedState};
use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use embassy_time::Timer;

/// Seconds between USB coming up and the deliberate panic.
///
/// Long enough to start a serial terminal by hand after the board reboots out of
/// DFU, since there is no way to ask the firmware to wait for one.
const PANIC_DELAY_SECS: u32 = 10;

// The shared panic handler, which is what this binary tests. `#[panic_handler]`
// must be expanded in the binary crate for the linker to find it, hence the
// wrapper. Same shape as main.rs.
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    asperitas_logging::panic_handler::handle_panic(info)
}

// Provides the _defmt_panic symbol that embassy-stm32's internal defmt usage
// requires. NOT the Rust panic handler. No `bkpt()`: with no debug probe
// attached it escalates to a HardFault instead of halting, which would discard
// the diagnostics this binary is trying to observe.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// No-op defmt logger. Must live in the binary crate — see the note in main.rs;
// the proc-macro only emits its linker symbols when expanded in the final binary.
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

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals — usb::init() steals them directly. Reading this
    // field would create a second Peri handle for the same peripheral. See the
    // longer note at the same line in main.rs.
    let _ = board.usb_peripherals;

    // Steady red from here on, until the countdown starts.
    led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);

    // Green during the countdown, so the panic handler's red is a change rather
    // than a continuation. Starting from red would make "panicked" and "still
    // booting" look the same.
    led::set_global_state(LedState::Running);

    let usb_fut = asperitas_logging::usb::run();
    let led_fut = led::blink_task();
    let countdown = async {
        for remaining in (1..=PANIC_DELAY_SECS).rev() {
            info!("panictest: panicking in {}...", remaining);
            Timer::after_secs(1).await;
        }
        panic!("panictest: deliberate panic, exercising the LED + serial panic path");
    };

    // All three are polled together: `usb_fut` must keep running for the
    // countdown's log lines to reach the host at all, and `led_fut` keeps the
    // LED state applied. The countdown never returns — it panics — so this
    // select does not complete and nothing follows it.
    embassy_futures::select::select3(usb_fut, led_fut, countdown).await;

    #[allow(clippy::empty_loop)]
    loop {}
}
