//! The Asperitas wire protocol, device and host sides.
//!
//! Everything here is `no_std` arithmetic on byte slices, with no transport and no clock:
//!
//! - [`frame`] - the framed record codec and its streaming [`frame::Decoder`]
//! - [`console`] - the device-to-host record bodies (boot, status, capture and rig records)
//! - [`dump`] - the audio-dump grammar and its strict base64 codec
//! - [`capture`] - the capture-ring geometry the dump and the rig price themselves against
//! - [`excerpt`] - the excerpt slot layout, install grammar and WAV parsing
//! - [`control`] - the host-to-device `CTL` command and `CTLR` reply grammar
//! - [`inbound`] - the owned record a validated host-to-device frame becomes
//!
//! The device-side runtime that ships these bytes (USB, the log ring, counters in statics)
//! lives in `asperitas-wire`, which depends on this crate and not the other way round.

#![no_std]

pub use log::Level;

pub mod capture;
pub mod console;
pub mod control;
pub mod dump;
pub mod excerpt;
pub mod frame;
pub mod inbound;

/// A `fmt::Write` that stops at the end of its buffer instead of overflowing it.
///
/// Log bodies may neither allocate nor panic, so running past the window has to be a
/// quiet stop rather than an error path. A piece that does not fit at all returns
/// [`core::fmt::Error`], which aborts the enclosing `write!` and leaves whatever already
/// landed - a shortened record, which the encoder then reports on the wire as `trunc`.
pub struct TruncWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> TruncWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes written so far.
    pub fn filled(&self) -> usize {
        self.pos
    }
}

impl core::fmt::Write for TruncWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
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
