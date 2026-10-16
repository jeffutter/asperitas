//! Load a recorded console capture and prove every dump block in it complete.
//!
//! Strictness matches `dump_reassemble`: a bad frame, a discarded byte, a refused or abandoned
//! block, or a malformed dump record is an error. A short sample vector that looks fine is the
//! failure this exists to prevent.

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use asperitas_wire::dump::{CHUNK_RAW, MAX_CHUNKS_PER_BLOCK};
use asperitas_wire::frame::Decoder;

use crate::assemble::{Action, BlockAssembler};
use crate::stream::pump;

/// Staging room for one maximum-size block, from the grammar.
const STAGING_BYTES: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// The PCM of every completed block, concatenated in completion order.
#[derive(Clone, Debug)]
pub struct ProvedPcm {
    pub pcm: Vec<u8>,
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
    /// No block completed.
    Empty,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "capture read failed: {e}"),
            LoadError::Zstd(e) => write!(f, "zstd decode failed: {e}"),
            LoadError::Unproved(m) => write!(f, "capture not fully proved: {m}"),
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

/// Load a zstd-compressed console capture from disk.
pub fn load_console_zst(path: impl AsRef<Path>) -> Result<ProvedPcm, LoadError> {
    let file = BufReader::new(File::open(path)?);
    let decoder = ruzstd::decoding::StreamingDecoder::new(file)
        .map_err(|e| LoadError::Zstd(e.to_string()))?;
    load_console(decoder)
}

/// Reassemble a raw (already decompressed) console byte stream.
pub fn load_console<R: Read>(input: R) -> Result<ProvedPcm, LoadError> {
    let mut decoder = Decoder::new();
    let mut staging = vec![0u8; STAGING_BYTES];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut pcm: Vec<u8> = Vec::new();
    let mut blocks = 0usize;
    let mut problems: Vec<String> = Vec::new();

    pump(input, &mut decoder, |record| {
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
                Action::Malformed { at } => problems.push(format!("malformed dump record at {at}")),
                _ => {}
            }
        }
    })
    .into_result()?;

    if let Some(a) = assembler.finish().abandoned {
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
    Ok(ProvedPcm { pcm, blocks })
}
