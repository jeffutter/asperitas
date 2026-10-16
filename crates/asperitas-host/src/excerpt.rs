//! The host half of putting an excerpt on the device: turning a WAV file into PCM bytes
//! ([`parse_wav`]) and PCM bytes into the framed install stream ([`install_frames`]).
//!
//! The slot layout, record grammar and the device-side [`Installer`](asperitas_wire::excerpt::Installer)
//! live in [`asperitas_wire::excerpt`]; [`install_frames`] is the only code that emits an install
//! stream and `Installer` the only code that consumes one, so the wire cannot drift between them.

use asperitas_wire::excerpt::{
    exc_data_body, exc_end_body, exc_start_body, LengthError, RecordError, Tag, EXC_CHUNK_RAW,
    WAV_SAMPLE_RATE,
};
use asperitas_wire::frame::{crc16_ccitt, encode, MAX_BODY, MAX_FRAME};
use asperitas_wire::Level;

/// Level every install frame carries. The device's inbound path ignores the level; a fixed one
/// keeps the stream byte-for-byte reproducible.
pub const INSTALL_LEVEL: Level = Level::Info;

/// Emit the whole install stream for `pcm` into `slot` under `name`: `EXCSTART`, one `EXCDATA`
/// per [`EXC_CHUNK_RAW`] bytes with `i` counting from 0, then `EXCEND`.
///
/// Each frame goes to `sink` whole, one call per frame, from a stack buffer: no allocation, no
/// clock. Sequence numbers count up from `seq0` and every frame carries `t_ms`, both from the
/// caller, so the same arguments always produce the same bytes. Returns the next unused sequence
/// number.
///
/// The arguments are checked before anything is emitted, so on `Err` the sink was never called.
pub fn install_frames<F: FnMut(&[u8])>(
    slot: u8,
    name: &Tag,
    pcm: &[u8],
    seq0: u32,
    t_ms: u32,
    mut sink: F,
) -> Result<u32, RecordError> {
    let bytes = u32::try_from(pcm.len())
        .map_err(|_| RecordError::Length(LengthError::OverCapacity { bytes: u32::MAX }))?;
    let mut body = [0u8; MAX_BODY];
    let mut frame = [0u8; MAX_FRAME];
    let mut seq = seq0;
    let mut emit = |body: &[u8], seq: &mut u32| {
        let len = encode(INSTALL_LEVEL, *seq, t_ms, body, &mut frame).len;
        sink(&frame[..len]);
        *seq = seq.wrapping_add(1);
    };

    let start_len = exc_start_body(slot, name, bytes, crc16_ccitt(pcm), &mut body)?;
    emit(&body[..start_len], &mut seq);
    for (index, chunk) in pcm.chunks(EXC_CHUNK_RAW).enumerate() {
        // Cannot fail: check_pcm_len above bounded pcm by PCM_CAPACITY, so every index is below
        // MAX_DATA_CHUNKS and every chunk is 1..=EXC_CHUNK_RAW bytes.
        let data_len = exc_data_body(index as u16, chunk, &mut body)?;
        emit(&body[..data_len], &mut seq);
    }
    let end_len = exc_end_body(&mut body);
    emit(&body[..end_len], &mut seq);
    Ok(seq)
}

/// Why a file cannot become an excerpt. Anything other than PCM mono 48 kHz 16-bit is refused
/// rather than converted: resampling or downmixing here would make the replay CRC describe audio
/// the host file does not contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    NotRiff,
    NotWave,
    /// The RIFF size, a chunk header, or a chunk body runs past the end of the input.
    Truncated,
    /// A `data` chunk appeared before any `fmt ` chunk, so its samples have no declared shape.
    DataBeforeFmt,
    /// No `data` chunk.
    MissingData,
    /// `fmt ` is shorter than the 16 bytes PCM needs.
    FmtTooShort {
        len: u32,
    },
    FormatTag {
        tag: u16,
    },
    Channels {
        channels: u16,
    },
    SampleRate {
        rate: u32,
    },
    BitsPerSample {
        bits: u16,
    },
    /// `block_align` or `byte_rate` disagree with mono 16-bit at 48 kHz.
    Inconsistent,
    EmptyData,
    OddData {
        len: u32,
    },
}

fn le16(raw: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([raw[at], raw[at + 1]])
}

fn le32(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]])
}

/// Return the PCM bytes of the first `data` chunk of a mono 48 kHz 16-bit PCM WAV.
///
/// Chunks are walked rather than assumed to sit at the canonical 44-byte offsets: `LIST`, `fact`
/// and anything else unknown are skipped, honouring the RIFF rule that odd-sized chunks carry a
/// pad byte. The walk is bounded by the RIFF size, which must itself fit the input.
pub fn parse_wav(raw: &[u8]) -> Result<&[u8], WavError> {
    if raw.len() < 12 || raw[0..4] != *b"RIFF" {
        return Err(WavError::NotRiff);
    }
    if raw[8..12] != *b"WAVE" {
        return Err(WavError::NotWave);
    }
    let riff_end = (le32(raw, 4) as usize)
        .checked_add(8)
        .ok_or(WavError::Truncated)?;
    let raw = raw.get(..riff_end).ok_or(WavError::Truncated)?;

    let mut at = 12usize;
    let mut fmt_seen = false;
    loop {
        if at == raw.len() {
            return Err(WavError::MissingData);
        }
        let header = raw.get(at..at + 8).ok_or(WavError::Truncated)?;
        let id = &header[0..4];
        let len = le32(header, 4);
        let body_at = at + 8;
        let body = raw
            .get(
                body_at
                    ..body_at
                        .checked_add(len as usize)
                        .ok_or(WavError::Truncated)?,
            )
            .ok_or(WavError::Truncated)?;

        if id == b"fmt " {
            if len < 16 {
                return Err(WavError::FmtTooShort { len });
            }
            let tag = le16(body, 0);
            let channels = le16(body, 2);
            let rate = le32(body, 4);
            let byte_rate = le32(body, 8);
            let block_align = le16(body, 12);
            let bits = le16(body, 14);
            if tag != 1 {
                return Err(WavError::FormatTag { tag });
            }
            if channels != 1 {
                return Err(WavError::Channels { channels });
            }
            if rate != WAV_SAMPLE_RATE {
                return Err(WavError::SampleRate { rate });
            }
            if bits != 16 {
                return Err(WavError::BitsPerSample { bits });
            }
            if block_align != 2 || byte_rate != WAV_SAMPLE_RATE * 2 {
                return Err(WavError::Inconsistent);
            }
            fmt_seen = true;
        } else if id == b"data" {
            if !fmt_seen {
                return Err(WavError::DataBeforeFmt);
            }
            if len == 0 {
                return Err(WavError::EmptyData);
            }
            if !len.is_multiple_of(2) {
                return Err(WavError::OddData { len });
            }
            return Ok(body);
        }

        // Odd-sized chunks are followed by one pad byte. The final chunk may omit it in files
        // written by careless tools; landing one past the end is then reported as truncation of a
        // chunk header rather than accepted.
        at = body_at + len as usize + (len as usize & 1);
        if at > raw.len() {
            return Err(WavError::Truncated);
        }
    }
}
