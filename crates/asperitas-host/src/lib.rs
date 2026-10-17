//! Host-side tooling for the Asperitas wire protocol.
//!
//! Everything here reads what the device sent or prepares what the host sends, so it uses `std`
//! freely; `asperitas-wire` stays `no_std` and the firmware links none of this.
//!
//! - [`stream`] - [`pump`](stream::pump), the read/push/drain loop every console reader shares
//! - [`assemble`] - [`BlockAssembler`](assemble::BlockAssembler), which proves audio-dump blocks complete
//! - [`capture`] - loading a recorded console capture into proved PCM
//! - [`timing`] - device-clock time of the first knob override in a recorded stream
//! - [`excerpt`] - WAV parsing and the framed excerpt install stream

pub mod assemble;
pub mod capture;
pub mod excerpt;
pub mod stream;
pub mod timing;
