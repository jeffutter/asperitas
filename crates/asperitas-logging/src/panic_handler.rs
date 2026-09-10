//! Custom panic handler with a steady red LED and, when a serial transport is
//! compiled in, a panic message on it.
//!
//! Replaces `panic-halt` with visible (LED) and — under `log-usb` — serial feedback on
//! panic. The LED is set unconditionally — a synchronous GPIO write that works even with no
//! host attached. The serial message is then pushed straight to the CDC endpoint by
//! `usb::emit_blocking` (the `log-usb` transport module), bounded by a timeout.
//!
//! With `boot-led` on and `log-usb` off this degrades to LED-then-halt, which is what a board
//! that dies before USB enumerates can still do: the colour is the whole diagnostic.
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
//! shared BootLed instance. The panic handler drives it through
//! [`crate::led::set_global_state`], which needs no initialized singleton to be safe.

use core::panic::PanicInfo;

// Both feed [`format_panic_message`], which exists only when some transport can carry its output.
// Ungated, a build with no transport at all warns about them.
#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
use core::fmt::Write;

#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
use crate::TruncWriter;

/// Size of the panic message buffer, in bytes.
///
/// A local on the panic stack, so it stays small on purpose: the alternative is a
/// 228-byte `MAX_FRAME` array, and this handler runs on a stack that may already be
/// nearly exhausted.
#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
const PANIC_MSG_BUF: usize = 128;

/// Handle a panic — called by the binary crate's `#[panic_handler]`.
///
/// This function:
/// 1. Sets the LED to red (panicked state) — always works, synchronous
/// 2. Under `log-usb`: writes the panic message over USB serial as one framed record,
///    driving the endpoint directly
/// 3. Halts
pub fn handle_panic(info: &PanicInfo) -> ! {
    // 1. Set LED to panicked state via the shared BootLed — synchronous, always works
    crate::led::set_global_state(crate::led::LedState::Panicked);

    // 2. Send the panic message over USB serial.
    //
    // Deliberately NOT through the log pipe. The pipe is drained by a future
    // inside `usb::run()`, and by the time we get here the async executor is
    // halted for good — so a pipe write is not "best effort", it is guaranteed
    // to be discarded. `usb::emit_panic_record` frames the text as a console v1
    // record and then drives the CDC endpoint itself, which works because the USB
    // interrupt is still firing during the spin below. It is time-bounded, takes
    // no lock, allocates nothing, and never panics.
    #[cfg(feature = "log-usb")]
    {
        let (msg, len) = format_panic_message(info);
        crate::usb::emit_panic_record(msg.get(..len).unwrap_or(&[][..]));
    }

    // Same text, second transport. Deliberately not through `log::`: the executor is halted, so a
    // record committed to any queue would be queued forever. `emit_panic` calls the defmt macro
    // directly, which writes the RTT ring synchronously — no task has to be alive for it to land.
    #[cfg(feature = "log-defmt")]
    {
        let (msg, len) = format_panic_message(info);
        crate::defmt_log::emit_panic(msg.get(..len).unwrap_or(&[][..]));
    }

    // Without a transport there is nothing left to deliver with the panic text: the colour
    // the LED was just driven to is the whole diagnostic, so `info` goes unread.
    #[cfg(not(any(feature = "log-usb", feature = "log-defmt")))]
    let _ = info;

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
#[cfg(any(feature = "log-usb", feature = "log-defmt"))]
fn format_panic_message(info: &PanicInfo) -> ([u8; PANIC_MSG_BUF], usize) {
    let mut buf = [0u8; PANIC_MSG_BUF];

    let mut w = TruncWriter::new(&mut buf);
    let _ = core::write!(w, "PANIC: {}", info.message());
    if let Some(loc) = info.location() {
        let _ = core::write!(w, " at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    // No trailing CRLF: this text becomes the *body* of a framed record, and the frame
    // supplies the delimiter. A CR or LF inside a body would only be neutralised to `_`
    // by the sanitiser, so the line break belongs to the framing and nowhere else.

    let len = w.filled();
    (buf, len)
}
