//! Logging facade for Asperitas firmware.
//!
//! Provides a feature-selected logging backend using the `log` crate.
//! Call-sites use `info!`, `debug!`, `warn!`, `error!` from the re-exported macros.
//! Backend selection is entirely at init time via Cargo features.
//!
//! # Features
//!
//! - `log-usb` — USB CDC-ACM serial logging over the Seed3's onboard USB-C
//! - (future) `log-defmt` — defmt-based logging for probe-based debugging
//!
//! When no backend feature is enabled, logging falls back to a no-op logger.

#![no_std]
#![allow(static_mut_refs)] // Safe on single-core Cortex-M with controlled access patterns

pub use log::{debug, error, info, trace, warn, Level, LevelFilter};

use core::sync::atomic::{AtomicBool, Ordering};

/// Has the logger been installed?
static LOGGER_INSTALLED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Global logger — implements log::Log
// ---------------------------------------------------------------------------

/// Backend enum — selected at compile time by features.
pub(crate) enum Backend {
    NoOp,
    #[cfg(feature = "log-usb")]
    Usb,
}

impl Backend {
    #[allow(unused_variables)]
    pub(crate) fn write(&self, bytes: &[u8]) -> bool {
        match self {
            Backend::NoOp => true,
            #[cfg(feature = "log-usb")]
            Backend::Usb => usb_pipe_write(bytes),
        }
    }
}

#[cfg(feature = "log-usb")]
fn usb_pipe_write(bytes: &[u8]) -> bool {
    use embassy_sync::pipe::TryWriteError;

    let pipe = unsafe { (*(&raw mut LOG_PIPE)).as_mut().unwrap() };
    match pipe.try_write(bytes) {
        Ok(_) => true,
        Err(TryWriteError::Full) => false,
    }
}

/// The global logger instance.
static mut GLOBAL_BACKEND: Backend = Backend::NoOp;

struct FacadeLogger;

impl log::Log for FacadeLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let len = format_log_record(record);
        // Safe: we only write to GLOBAL_BACKEND during init (single-threaded boot)
        // and after that it's read-only.
        unsafe {
            let _ = GLOBAL_BACKEND.write(FORMAT_BUF.get(..len).unwrap_or(&[][..]));
        }
    }

    fn flush(&self) {}
}

/// Thread-local-format buffer for log records.
static mut FORMAT_BUF: [u8; 256] = [0; 256];

/// Format a log record into the static buffer, returning the number of bytes written.
fn format_log_record(record: &log::Record) -> usize {
    use core::fmt::Write;

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

    unsafe {
        let buf_ptr = &raw mut FORMAT_BUF;
        let mut w = Writer {
            buf: &mut *buf_ptr,
            pos: 0,
        };
        let _ = core::write!(w, "[{}] {}\r\n", record.level(), record.args());
        w.pos
    }
}

/// Install the facade logger as the global `log` backend.
pub(crate) fn install_logger() {
    if LOGGER_INSTALLED.swap(true, Ordering::AcqRel) {
        return; // Already installed
    }

    log::set_logger(&FacadeLogger)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .ok();
}

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "log-usb")]
pub mod usb;

#[cfg(feature = "log-usb")]
pub mod led;

#[cfg(feature = "log-usb")]
pub mod panic_handler;

// ---------------------------------------------------------------------------
// USB pipe — shared between lib.rs, usb.rs, and panic_handler.rs
// ---------------------------------------------------------------------------

/// Pipe buffer size — holds several log messages before backpressure.
#[cfg(feature = "log-usb")]
pub const LOG_PIPE_SIZE: usize = 512;

/// Log pipe storage. Initialized once by [`usb::init`].
#[cfg(feature = "log-usb")]
pub static mut LOG_PIPE: Option<
    embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, LOG_PIPE_SIZE>,
> = None;

/// Get a reference to the initialized log pipe.
/// Panics if USB hasn't been initialized yet.
#[cfg(feature = "log-usb")]
pub fn pipe() -> &'static mut embassy_sync::pipe::Pipe<
    embassy_sync::blocking_mutex::raw::NoopRawMutex,
    LOG_PIPE_SIZE,
> {
    // Safe: we only write to LOG_PIPE during init (single-threaded boot),
    // and after that the Option is never changed. The Pipe's internal mutex
    // handles concurrent access safely.
    unsafe { (*(&raw mut LOG_PIPE)).as_mut().unwrap() }
}

// ---------------------------------------------------------------------------
// Default init (no features)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "log-usb"))]
/// Initialize the logging facade with a no-op backend.
///
/// When no backend feature is enabled, all log messages are silently dropped.
pub fn init() {
    install_logger();
}
