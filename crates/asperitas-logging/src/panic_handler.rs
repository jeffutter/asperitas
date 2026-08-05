//! Custom panic handler with a steady red LED and a USB serial message.
//!
//! Replaces `panic-halt` with visible (LED) and serial feedback on panic. The LED
//! is set unconditionally — a synchronous GPIO write that works even with no host
//! attached. The serial message is then pushed straight to the CDC endpoint by
//! [`crate::usb::emit_blocking`], bounded by a timeout.
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
/// 2. Writes the panic message over USB serial, driving the endpoint directly
/// 3. Halts
pub fn handle_panic(info: &PanicInfo) -> ! {
    // 1. Set LED to panicked state via the shared BootLed — synchronous, always works
    crate::led::set_global_state(crate::led::LedState::Panicked);

    // 2. Send the panic message over USB serial.
    //
    // Deliberately NOT through the log pipe. The pipe is drained by a future
    // inside `usb::run()`, and by the time we get here the async executor is
    // halted for good — so a pipe write is not "best effort", it is guaranteed
    // to be discarded. `usb::emit_blocking` drives the USB device and the CDC
    // endpoint itself, which works because the USB interrupt is still firing
    // during the spin below. It is time-bounded and never panics.
    let (msg, len) = format_panic_message(info);
    crate::usb::emit_blocking(msg.get(..len).unwrap_or(&[][..]));

    // 3. Halt.
    //
    // No `bkpt()` here. BKPT only halts when a debugger is attached; with none
    // present it escalates to a HardFault, whose default handler is its own
    // infinite loop. That would discard the red LED and the pipe write above —
    // the two things this handler exists to deliver — and leave a board that
    // looks simply dead. This project has no debug probe (see the README), so
    // a plain spin is the behaviour that actually preserves the diagnostics.
    loop {
        cortex_m::asm::nop();
    }
}

/// Format panic information into a fixed-size buffer.
///
/// Returns the buffer and the number of bytes actually written. The length
/// matters: the buffer is zero-filled, so writing all of it emits the message
/// followed by NUL padding, which shows up as garbage on a serial terminal.
fn format_panic_message(info: &PanicInfo) -> ([u8; 128], usize) {
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
    // CRLF, not LF: this lands in a raw serial terminal, which does not translate
    // a bare newline into a carriage return. Matches `format_log_record`.
    let _ = core::write!(w, "\r\n");

    let len = w.pos;
    (buf, len)
}
