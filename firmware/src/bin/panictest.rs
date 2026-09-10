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
//! With a probe instead of a cable (`make probe-run BINARY=panictest FEATURES="seed3 log-defmt"
//! NO_DEFAULT=1`) the same stages arrive as defmt frames on RTT and the LED column is unchanged.
//! Attach *before* the run starts: defmt-rtt trims writes while nothing is reading, so a
//! countdown that happened unattached may never appear. The upside is that this variant also
//! shows the pre-USB window, which the console cannot reach at all.
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

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Install the backend before anything can log; with the console compiled in, `usb::init`
    // below does it instead.
    #[cfg(not(feature = "log-usb"))]
    asperitas_logging::init();

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);
    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals — usb::init() steals them directly. Reading this
    // field would create a second Peri handle for the same peripheral. See the
    // longer note at the same line in main.rs.
    let _ = board.usb_peripherals;

    // Steady red from here on, until the countdown starts.
    led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    #[cfg(feature = "log-usb")]
    let _usb_handle = asperitas_logging::usb::init(UsbIrqs);

    // Green during the countdown, so the panic handler's red is a change rather
    // than a continuation. Starting from red would make "panicked" and "still
    // booting" look the same.
    led::set_global_state(LedState::Running);

    #[cfg(feature = "log-usb")]
    let console_fut = asperitas_logging::usb::run();
    // No drain task without the console; RTT is read out of RAM by the probe. See main.rs.
    #[cfg(not(feature = "log-usb"))]
    let console_fut = core::future::pending::<()>();
    let led_fut = led::blink_task();
    let countdown = async {
        for remaining in (1..=PANIC_DELAY_SECS).rev() {
            info!("panictest: panicking in {}...", remaining);
            Timer::after_secs(1).await;
        }
        panic!("panictest: deliberate panic, exercising the LED + serial panic path");
    };

    // All three are polled together: `console_fut` must keep running for the
    // countdown's log lines to reach the host at all, and `led_fut` keeps the
    // LED state applied. The countdown never returns — it panics — so this
    // select does not complete and nothing follows it.
    embassy_futures::select::select3(console_fut, led_fut, countdown).await;

    #[allow(clippy::empty_loop)]
    loop {}
}
