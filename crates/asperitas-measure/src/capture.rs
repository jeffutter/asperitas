//! Load a rig capture from the framed console stream as mono 16-bit samples.
//!
//! Reassembly and its strictness live in [`asperitas_host::capture`]: a bad frame, a discarded
//! byte, a refused or abandoned block, or a malformed dump record is an error. A short sample
//! vector that looks fine is the failure that exists to prevent.

use std::fmt;
use std::io::Read;
use std::path::Path;

use asperitas_host::capture::{load_console, load_console_zst, ProvedPcm};

pub use asperitas_wire::capture::SAMPLE_RATE_HZ;

/// Magnitude of the 16-bit scale: `i16::MIN` is `-FULL_SCALE`. Matches the divisor of
/// [`asperitas_dsp::pcm::i16_to_f32`].
pub const FULL_SCALE: f64 = 32768.0;

/// A fully proved capture: mono 16-bit samples in block-completion order.
#[derive(Clone, Debug)]
pub struct Capture {
    pub samples: Vec<i16>,
    pub sample_rate_hz: u32,
    /// Number of blocks proved complete.
    pub blocks: usize,
}

/// Why a capture could not be loaded as a whole.
#[derive(Debug)]
pub enum LoadError {
    /// The console stream could not be read or did not prove out.
    Console(asperitas_host::capture::LoadError),
    /// Completed blocks carried an odd number of PCM bytes.
    OddPcm(usize),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Console(e) => e.fmt(f),
            LoadError::OddPcm(n) => {
                write!(f, "PCM byte count {n} is not a whole number of samples")
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl From<asperitas_host::capture::LoadError> for LoadError {
    fn from(e: asperitas_host::capture::LoadError) -> Self {
        LoadError::Console(e)
    }
}

impl Capture {
    /// The samples scaled into `[-1.0, 1.0)`.
    pub fn normalized(&self) -> Vec<f32> {
        self.samples
            .iter()
            .map(|&s| asperitas_dsp::pcm::i16_to_f32(s))
            .collect()
    }

    /// Load a zstd-compressed console capture from disk.
    pub fn from_console_zst(path: impl AsRef<Path>) -> Result<Capture, LoadError> {
        Capture::from_proved(load_console_zst(path)?)
    }

    /// Reassemble a raw (already decompressed) console byte stream.
    pub fn from_console_reader<R: Read>(input: R) -> Result<Capture, LoadError> {
        Capture::from_proved(load_console(input)?)
    }

    fn from_proved(proved: ProvedPcm) -> Result<Capture, LoadError> {
        let ProvedPcm { pcm, blocks } = proved;
        if !pcm.len().is_multiple_of(2) {
            return Err(LoadError::OddPcm(pcm.len()));
        }
        let samples = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        Ok(Capture {
            samples,
            sample_rate_hz: SAMPLE_RATE_HZ,
            blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../audio/captures/2026-10-07-rig-sine-300s.console.zst"
    );

    #[test]
    fn rig_sine_300s_loads_whole() {
        let c = Capture::from_console_zst(FIXTURE).expect("fixture loads");
        assert_eq!(c.samples.len(), 14_401_536);
        assert_eq!(c.blocks, 879);
        assert_eq!(c.sample_rate_hz, 48_000);
    }

    #[test]
    fn a_stream_that_does_not_prove_is_an_error_not_an_empty_capture() {
        let err = Capture::from_console_reader(&[0xA5u8; 300][..]).unwrap_err();
        assert!(matches!(err, LoadError::Console(_)), "{err}");
    }
}
