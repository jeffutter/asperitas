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
//! #[panic_handler]
//! fn panic_handler(info: &core::panic::PanicInfo) -> ! {
//!     asperitas_logging::panic_handler::handle_panic(info)
//! }
//! ```
//!
//! Before any panic can occur, call [`crate::led::init`] to initialize the
//! shared BootLed instance. The panic handler uses the same singleton via
//! [`crate::led::get_mut`].

use core::fmt::Write;
use core::panic::PanicInfo;

/// Handle a panic — called by the binary crate's `#[panic_handler]`.
///
/// This function:
/// 1. Sets the LED to red (panicked state) — always works, synchronous
/// 2. Attempts to write the panic message over USB serial — best effort
/// 3. Enters an infinite loop with bkpt
pub fn handle_panic(info: &PanicInfo) -> ! {
    // 1. Set LED to panicked state via the shared BootLed — synchronous, always works
    crate::led::set_global_state(crate::led::LedState::Panicked);

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
