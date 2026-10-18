//! The Asperitas wire protocol, device and host sides.
//!
//! Everything here is `no_std` arithmetic on byte slices, with no transport and no clock:
//!
//! - [`frame`] - the framed record codec and its streaming [`frame::Decoder`]
//! - [`console`] - the device-to-host record bodies (boot, status, capture and rig records)
//! - [`dump`] - the audio-dump grammar and its strict base64 codec
//! - [`capture`] - the capture-ring geometry the dump and the rig price themselves against
//! - [`excerpt`] - the excerpt slot layout, install grammar and the device-side installer
//! - [`control`] - the host-to-device `CTL` command and `CTLR` reply grammar
//! - [`inbound`] - the owned record a validated host-to-device frame becomes
//!
//! `docs/reference/protocol.md` is the one-page index from each grammar to the module that owns it.
//!
//! The device-side runtime that ships these bytes (USB, the log ring, counters in statics)
//! lives in `asperitas-logging`, which depends on this crate and not the other way round. The
//! host-only halves - block assembly, WAV parsing, the install stream - live in `asperitas-host`.

#![no_std]

pub use log::Level;

pub mod body;
pub mod capture;
pub mod console;
pub mod control;
pub mod counter;
pub mod dump;
pub mod excerpt;
pub mod frame;
pub mod inbound;
