//! Custom panic handler with LED strobe and optional USB serial message.
//!
//! Replaces `panic-halt` with visible (LED) and serial feedback on panic.
//! The LED state is always set; USB write is attempted if available.
//!
//! # Usage
//!
//! In your binary crate's `main.rs`:
//!
//! ```ignore
//! use asperitas_logging::panic_handler as _;
//! ```
//!
//! The `as _` ensures the `#[panic_handler]` attribute is registered without
//! creating an unused-import warning.
//!
//! Before any panic can occur, call [`set_panic_led`] to install the LED
//! reference that the panic handler will use.

use core::fmt::Write;
use core::panic::PanicInfo;
use embassy_stm32::{self as hal, Peri};

/// Global LED state for the panic handler.
/// Set by [`set_panic_led`] during initialization.
struct PanicLedState {
    red: hal::gpio::Output<'static>,
    green: hal::gpio::Output<'static>,
    blue: hal::gpio::Output<'static>,
}

static mut PANIC_LED: Option<PanicLedState> = None;

/// Install the LED reference for the panic handler.
///
/// Call this after creating the boot LED pins but before any panic can occur.
/// The panic handler will use these GPIO pins to signal a panicked state.
///
/// # Arguments
///
/// * `red_pin` — Red channel pin (Pod D20 = PC1)
/// * `green_pin` — Green channel pin (Pod D19 = PA6)
/// * `blue_pin` — Blue channel pin (Pod D18 = PA7)
pub fn set_panic_led(
    red_pin: Peri<'static, hal::peripherals::PC1>,
    green_pin: Peri<'static, hal::peripherals::PA6>,
    blue_pin: Peri<'static, hal::peripherals::PA7>,
) {
    let off = if super::led::LED_ACTIVE_LOW {
        hal::gpio::Level::High
    } else {
        hal::gpio::Level::Low
    };

    let led = PanicLedState {
        red: hal::gpio::Output::new(red_pin, off, hal::gpio::Speed::Low),
        green: hal::gpio::Output::new(green_pin, off, hal::gpio::Speed::Low),
        blue: hal::gpio::Output::new(blue_pin, off, hal::gpio::Speed::Low),
    };

    cortex_m::interrupt::free(|_| unsafe {
        PANIC_LED = Some(led);
    });
}

/// The custom panic handler.
///
/// This function replaces `panic-halt`. It:
/// 1. Sets the LED to red (panicked state) — always works, synchronous
/// 2. Attempts to write the panic message over USB serial — best effort
/// 3. Enters an infinite loop with bkpt
//
// NOTE: This attribute registers the function as the panic handler for the
// binary crate that re-exports it. It can only appear once per binary.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    // 1. Set LED to red (panicked state) — synchronous, always works
    cortex_m::interrupt::free(|_| unsafe {
        if let Some(ref mut led) = PANIC_LED {
            let on = if super::led::LED_ACTIVE_LOW {
                hal::gpio::Level::Low
            } else {
                hal::gpio::Level::High
            };
            let off = if super::led::LED_ACTIVE_LOW {
                hal::gpio::Level::High
            } else {
                hal::gpio::Level::Low
            };

            led.red.set_level(on);
            led.green.set_level(off);
            led.blue.set_level(off);
        }
    });

    // 2. Try to write panic message over USB pipe (best-effort, non-blocking)
    // During panic, the async executor is halted, so we can't use embassy-usb's
    // async API. We attempt a synchronous pipe write. If the pipe is full or
    // USB isn't initialized, the message is silently dropped.
    let msg = format_panic_message(info);
    unsafe {
        if let Some(ref mut pipe) = *(&raw mut crate::LOG_PIPE) {
            let _ = pipe.try_write(&msg);
        }
    }

    // 3. Enter infinite loop
    cortex_m::asm::bkpt();
    loop {
        cortex_m::asm::nop();
    }
}

/// Format panic information into a fixed-size buffer.
fn format_panic_message(info: &PanicInfo) -> [u8; 128] {
    let mut buf = [0u8; 128];

    struct Writer<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl Write for Writer<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let available = self.buf.len() - self.pos;
            let len = s.len().min(available);
            if len == 0 {
                return Err(core::fmt::Error);
            }
            self.buf[self.pos..self.pos + len].copy_from_slice(&s.as_bytes()[..len]);
            self.pos += len;
            Ok(())
        }
    }

    let mut w = Writer {
        buf: &mut buf,
        pos: 0,
    };
    let _ = core::write!(w, "PANIC: {}", info.message());
    if let Some(loc) = info.location() {
        let _ = core::write!(w, " at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    let _ = core::write!(w, "\n");

    buf
}
