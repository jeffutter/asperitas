//! Load a rig capture from the framed console stream and refuse anything not fully proved.
//!
//! The reassembly loop deliberately mirrors `asperitas-logging`'s `dump_reassemble` example (as
//! `asperitas-rig` does) instead of exposing a `std` API from the firmware-shared crate. The
//! strictness is the same: a bad frame, a discarded byte, a refused or abandoned block, or a
//! malformed dump record is an error. A short sample vector that looks fine is the failure this
//! exists to prevent.

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use asperitas_logging::dump::{Action, BlockAssembler, CHUNK_RAW, MAX_CHUNKS_PER_BLOCK};
use asperitas_logging::frame::Decoder;

/// Bytes requested per read. Larger than one frame so record boundaries never align by luck.
const CHUNK_BYTES: usize = 4096;

/// Staging room for one maximum-size block, from the grammar.
const STAGING_BYTES: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// Rig capture sample rate, Hz.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

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
    Io(io::Error),
    /// The zstd layer failed to decode.
    Zstd(String),
    /// The stream was damaged or incomplete; the message gives the counts.
    Unproved(String),
    /// Completed blocks carried an odd number of PCM bytes.
    OddPcm(usize),
    /// No block completed.
    Empty,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "capture read failed: {e}"),
            LoadError::Zstd(e) => write!(f, "zstd decode failed: {e}"),
            LoadError::Unproved(m) => write!(f, "capture not fully proved: {m}"),
            LoadError::OddPcm(n) => {
                write!(f, "PCM byte count {n} is not a whole number of samples")
            }
            LoadError::Empty => write!(f, "capture contains no complete block"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl Capture {
    /// Load a zstd-compressed console capture from disk.
    pub fn from_console_zst(path: impl AsRef<Path>) -> Result<Capture, LoadError> {
        let file = BufReader::new(File::open(path)?);
        let decoder = ruzstd::decoding::StreamingDecoder::new(file)
            .map_err(|e| LoadError::Zstd(e.to_string()))?;
        Capture::from_console_reader(decoder)
    }

    /// Reassemble a raw (already decompressed) console byte stream.
    pub fn from_console_reader<R: Read>(mut input: R) -> Result<Capture, LoadError> {
        let mut decoder = Decoder::new();
        let mut staging = vec![0u8; STAGING_BYTES];
        let mut assembler = BlockAssembler::new(&mut staging);
        let mut chunk = [0u8; CHUNK_BYTES];
        let mut pcm: Vec<u8> = Vec::new();
        let mut blocks = 0usize;
        let mut problems: Vec<String> = Vec::new();

        let mut drain = |decoder: &mut Decoder, assembler: &mut BlockAssembler<'_>| {
            while let Some(record) = decoder.next_record() {
                let actions = assembler.accept(record.body);
                for action in actions.as_slice() {
                    match *action {
                        Action::Complete { bytes, .. } => {
                            let p = assembler.pcm();
                            debug_assert_eq!(p.len(), bytes);
                            pcm.extend_from_slice(p);
                            blocks += 1;
                        }
                        Action::Failed { block, reason } => {
                            problems.push(format!("block {block} refused: {reason:?}"))
                        }
                        Action::Abandoned { block, .. } => {
                            problems.push(format!("block {block} abandoned"))
                        }
                        Action::Conflict { block, chunk } => {
                            problems.push(format!("block {block} chunk {chunk} conflicts"))
                        }
                        Action::Malformed { at } => {
                            problems.push(format!("malformed dump record at {at}"))
                        }
                        _ => {}
                    }
                }
            }
        };

        loop {
            let filled = match input.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            let mut offset = 0usize;
            while offset < filled {
                let taken = decoder.push(&chunk[offset..filled]);
                offset += taken;
                drain(&mut decoder, &mut assembler);
                if taken == 0 {
                    return Err(LoadError::Unproved("decoder stopped taking input".into()));
                }
            }
        }
        decoder.finish();
        drain(&mut decoder, &mut assembler);
        let finish = assembler.finish();
        if let Some(a) = finish.abandoned {
            problems.push(format!("block {} abandoned at end of stream", a.block));
        }
        let stats = decoder.stats();
        if stats.bad_frames > 0 || stats.discarded_bytes > 0 {
            problems.push(format!(
                "{} bad frames, {} discarded bytes",
                stats.bad_frames, stats.discarded_bytes
            ));
        }
        if !problems.is_empty() {
            let shown = problems
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            return Err(LoadError::Unproved(format!(
                "{} problems ({} blocks proved): {shown}",
                problems.len(),
                blocks
            )));
        }
        if blocks == 0 {
            return Err(LoadError::Empty);
        }
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
    use asperitas_logging::dump::audio_record;
    use asperitas_logging::frame::MAX_FRAME;
    use asperitas_logging::Level;

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
    fn garbage_is_an_error_not_an_empty_capture() {
        let err = Capture::from_console_reader(&[0xA5u8; 300][..]).unwrap_err();
        assert!(
            matches!(err, LoadError::Unproved(_) | LoadError::Empty),
            "{err}"
        );
    }

    #[test]
    fn empty_stream_is_an_error() {
        assert!(matches!(
            Capture::from_console_reader(&[][..]),
            Err(LoadError::Empty)
        ));
    }

    const CHUNKS: u16 = 6;

    /// Wire bytes of a six-chunk block whose PCM is the ramp starting at `base`.
    fn block(seq: &mut u32, id: u32, base: i16, drop_chunk: Option<u16>) -> (Vec<u8>, Vec<i16>) {
        let samples: Vec<i16> = (0..(CHUNK_RAW as i16 * CHUNKS as i16 / 2))
            .map(|i| base.wrapping_add(i))
            .collect();
        let raw: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wire = Vec::new();
        for index in 0..CHUNKS {
            let mut body = [0u8; asperitas_logging::frame::MAX_BODY];
            let mut fb = [0u8; MAX_FRAME];
            let start = index as usize * CHUNK_RAW;
            let e = audio_record(
                Level::Info,
                *seq,
                *seq * 4,
                id,
                CHUNKS,
                index,
                &raw[start..start + CHUNK_RAW],
                &mut body,
                &mut fb,
            )
            .unwrap();
            *seq += 1;
            if Some(index) != drop_chunk {
                wire.extend_from_slice(&fb[..e.len]);
            }
        }
        let mut body = [0u8; asperitas_logging::frame::MAX_BODY];
        let mut fb = [0u8; MAX_FRAME];
        let e = asperitas_logging::dump::audend_record(
            Level::Info,
            *seq,
            *seq * 4,
            id,
            CHUNKS,
            raw.len() as u32,
            asperitas_logging::frame::crc16_ccitt(&raw),
            &mut body,
            &mut fb,
        )
        .unwrap();
        *seq += 1;
        wire.extend_from_slice(&fb[..e.len]);
        (wire, samples)
    }

    #[test]
    fn synthetic_stream_round_trips() {
        let mut seq = 0;
        let (a, sa) = block(&mut seq, 0, 0, None);
        let (b, sb) = block(&mut seq, 1, 1000, None);
        let c = Capture::from_console_reader([a, b].concat().as_slice()).unwrap();
        assert_eq!(c.blocks, 2);
        assert_eq!(c.samples, [sa, sb].concat());
    }

    #[test]
    fn dropped_chunk_is_an_error() {
        let mut seq = 0;
        let (a, _) = block(&mut seq, 0, 0, None);
        let (b, _) = block(&mut seq, 1, 1000, Some(1));
        let (d, _) = block(&mut seq, 2, 2000, None);
        let err = Capture::from_console_reader([a, b, d].concat().as_slice()).unwrap_err();
        assert!(matches!(err, LoadError::Unproved(_)), "{err}");
    }

    #[test]
    fn flipped_byte_is_an_error() {
        let mut seq = 0;
        let (mut a, _) = block(&mut seq, 0, 0, None);
        let mid = a.len() / 2;
        a[mid] ^= 0xff;
        let err = Capture::from_console_reader(a.as_slice()).unwrap_err();
        assert!(
            matches!(err, LoadError::Unproved(_) | LoadError::Empty),
            "{err}"
        );
    }
}
