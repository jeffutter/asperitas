//! Instrument excerpts in QSPI flash: where they live, how a slot describes itself, which WAV files
//! may become one, and the console records that carry one from the host to the device.
//!
//! Everything here is byte arithmetic with no hardware type in sight, so the decisions that would
//! otherwise be discovered at the bench - a write that erases its neighbour, a header that reads as
//! valid on blank flash, a WAV that is stereo or 44.1 kHz, a record body one byte over the frame
//! budget - are settled on the host. The install state machine (TASK-038.04.04) and the device's
//! flash task (TASK-038.04.05) consume this module; neither restates a number it owns.
//!
//! # Why this module is not behind `log-usb`
//!
//! Same reason as [`crate::dump`]: pure logic, tested under default features, linked by firmware
//! only where something calls it.
//!
//! # Storage layout (normative - TASK-038.04.06 mirrors this table into `docs/reference/daisy-seed3.md`)
//!
//! | region | range | why |
//! |---|---|---|
//! | DaisyBootloader | `0x000000..0x040000` | Untouched, so installing the bootloader later never moves an excerpt. |
//! | unused | `0x040000..0x100000` | Headroom between the bootloader and the excerpt area. |
//! | slot `n` (0..14) | `0x100000 + n * 0x80000`, [`SLOT_SPAN`] bytes | First sector holds the [`SlotHeader`], PCM starts one sector in. |
//! | slot guard | the last sector of every stride | Never written. In slot 13 it *is* the reserved top sector. |
//! | reserved | `0x7FF000..0x800000` | `daisy_embassy::flash` bounds every access by `address + len <= 0x7FFFFF`, so byte `0x7FFFFF` is unreachable and its whole sector is left alone. |
//!
//! A 512 KiB stride from `0x100000` puts fourteen strides exactly at `0x800000`, one sector past
//! what the driver allows. Rather than shorten the last slot alone, every slot gives up its top
//! sector: all fourteen slots then share one capacity, [`PCM_CAPACITY`] = 516,096 bytes (5.38 s of
//! mono 16-bit at 48 kHz, against a largest corpus clip of 288,000 bytes), and the guard sector of
//! slot 13 lands precisely on the reserved one.
//!
//! The driver erases the 4 KiB sector containing every address it writes. Every slot base, every
//! PCM base and [`SECTOR`] itself are therefore sector-aligned (const-asserted below), so a writer
//! that only ever writes whole sectors at those addresses cannot reach outside its slot.
//!
//! # Slot header
//!
//! [`HEADER_LEN`] bytes at the slot base, little-endian:
//!
//! | offset | width | field |
//! |---|---|---|
//! | 0 | 4 | magic `ASPX` |
//! | 4 | 2 | version, [`HEADER_VERSION`] |
//! | 6 | 16 | name tag, NUL-padded |
//! | 22 | 4 | PCM byte length |
//! | 26 | 2 | CRC-16/CCITT-FALSE of the PCM bytes |
//! | 28 | 2 | CRC-16/CCITT-FALSE of bytes `0..28` |
//!
//! Erased flash reads `0xFF`, which fails the magic; a header torn by a power cut fails its own
//! CRC. The PCM CRC is the value replay compares its DAC-path CRC against, so a slot carries the
//! proof of its contents with it.
//!
//! # Record grammar (normative)
//!
//! Host-to-device bodies riding [`crate::frame`] untouched:
//!
//! ```text
//! EXCSTART slot=<2 hex> name=<tag> bytes=<6 dec> crc16=<4 hex>
//! EXCDATA i=<4 hex> b64=<base64>
//! EXCEND
//! ```
//!
//! | field | meaning |
//! |---|---|
//! | `slot` | Target slot, `0..SLOT_COUNT`. TASK-038.04's AC #2 omits it, but the device has to be told where to write; deriving a slot from the name would mean scanning headers before every install. |
//! | `name` | 1 to [`TAG_MAX`] characters from `[a-z0-9_]`. The charset is what keeps `<` `>` (reserved host-to-device by TASK-032), `~` (the record start marker) and space (the field separator) out of the body. |
//! | `bytes` | PCM length, fixed width like `dump`'s `bytes`. Even, non-zero and at most [`PCM_CAPACITY`]. |
//! | `crc16` | CRC-16/CCITT-FALSE over the PCM bytes, the same function the frame trailer and `AUDEND` use. |
//! | `i` | Chunk index. Chunk `i` carries PCM bytes `i * EXC_CHUNK_RAW ..`. |
//! | `b64` | Canonical base64 ([`crate::dump::encode`]) of 1 to [`EXC_CHUNK_RAW`] raw bytes. |
//!
//! [`EXC_CHUNK_RAW`] is 128 rather than the 135 the body budget would allow: 128 divides a sector
//! exactly, so 32 chunks fill one sector and a chunk never straddles two, and a chunk is always
//! whole samples. The cost is 7 raw bytes per record.
//!
//! What this module deliberately does not decide is sequencing - whether `i` arrived in order,
//! whether the chunks add up to `bytes`. That is the install state machine's job; a parsed record
//! here is only guaranteed to be well-formed and in range.

use crate::dump::{
    decode, encode, encoded_len, put, put_decimal, put_hex, BodyReader, DecodeError,
};
use crate::frame::{check_decimal_fits, check_hex_width, crc16_ccitt, MAX_BODY};

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Highest address `daisy_embassy::flash` accepts as `address + len`. Mirrored rather than
/// imported because this crate links no board support; the driver asserts the same bound.
pub const MAX_ADDRESS: u32 = 0x7F_FFFF;

/// Erase granularity of the IS25LP064A as the driver uses it: 4 KiB sectors, nothing larger.
pub const SECTOR: u32 = 4096;

/// First byte of the sector the driver's bound makes partly unreachable, so nothing uses it.
pub const RESERVED_SECTOR_START: u32 = (MAX_ADDRESS + 1) - SECTOR;

/// End of the DaisyBootloader's region; excerpts never start below it.
pub const BOOTLOADER_END: u32 = 0x4_0000;

/// Start of slot 0.
pub const EXCERPT_AREA_START: u32 = 0x10_0000;

/// Distance between consecutive slot bases: 512 KiB.
pub const SLOT_STRIDE: u32 = 0x8_0000;

/// Number of slots.
pub const SLOT_COUNT: u8 = 14;

/// Bytes of a stride a slot may touch: the stride minus its guard sector.
pub const SLOT_SPAN: u32 = SLOT_STRIDE - SECTOR;

/// Offset of the PCM from its slot base: one sector, owned by the header.
pub const PCM_OFFSET: u32 = SECTOR;

/// Most PCM bytes one slot holds: 516,096.
pub const PCM_CAPACITY: u32 = SLOT_SPAN - PCM_OFFSET;

/// One past the last byte any slot may touch.
pub const EXCERPT_AREA_END: u32 =
    EXCERPT_AREA_START + (SLOT_COUNT as u32 - 1) * SLOT_STRIDE + SLOT_SPAN;

const _: () = assert!(EXCERPT_AREA_START >= BOOTLOADER_END);
const _: () = assert!(EXCERPT_AREA_END <= RESERVED_SECTOR_START);
const _: () = assert!(EXCERPT_AREA_END - 1 <= MAX_ADDRESS - SECTOR);
const _: () = assert!(EXCERPT_AREA_START.is_multiple_of(SECTOR));
const _: () = assert!(SLOT_STRIDE.is_multiple_of(SECTOR));
const _: () = assert!(PCM_OFFSET.is_multiple_of(SECTOR));
const _: () = assert!(PCM_CAPACITY.is_multiple_of(SECTOR));
const _: () = assert!(PCM_CAPACITY == 516_096);
// The last slot's guard sector is the reserved sector, not a second one beside it.
const _: () = assert!(EXCERPT_AREA_END == RESERVED_SECTOR_START);

/// Address of `slot`'s first byte (its header sector), or `None` for a slot that does not exist.
pub const fn slot_base(slot: u8) -> Option<u32> {
    if slot >= SLOT_COUNT {
        return None;
    }
    Some(EXCERPT_AREA_START + slot as u32 * SLOT_STRIDE)
}

/// Address of `slot`'s first PCM byte, or `None` for a slot that does not exist.
pub const fn slot_pcm_base(slot: u8) -> Option<u32> {
    match slot_base(slot) {
        Some(base) => Some(base + PCM_OFFSET),
        None => None,
    }
}

/// Why a PCM byte count cannot describe an excerpt. One rule, shared by the slot header and the
/// `EXCSTART` record, so the two cannot disagree about what fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthError {
    /// No samples at all.
    Empty,
    /// Not a whole number of 16-bit samples.
    Odd { bytes: u32 },
    /// More than [`PCM_CAPACITY`].
    OverCapacity { bytes: u32 },
}

/// Accept `bytes` as an excerpt length: non-zero, even, within one slot.
pub const fn check_pcm_len(bytes: u32) -> Result<(), LengthError> {
    if bytes == 0 {
        Err(LengthError::Empty)
    } else if !bytes.is_multiple_of(2) {
        Err(LengthError::Odd { bytes })
    } else if bytes > PCM_CAPACITY {
        Err(LengthError::OverCapacity { bytes })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Name tag
// ---------------------------------------------------------------------------

/// Longest name tag, in bytes.
pub const TAG_MAX: usize = 16;

/// Why a name was refused as a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagError {
    Empty,
    TooLong {
        len: usize,
    },
    /// `byte` at `at` is outside `[a-z0-9_]`.
    Char {
        at: usize,
        byte: u8,
    },
}

/// A validated excerpt name: 1 to [`TAG_MAX`] bytes of `[a-z0-9_]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    bytes: [u8; TAG_MAX],
    len: u8,
}

impl Tag {
    pub fn new(name: &[u8]) -> Result<Self, TagError> {
        if name.is_empty() {
            return Err(TagError::Empty);
        }
        if name.len() > TAG_MAX {
            return Err(TagError::TooLong { len: name.len() });
        }
        if let Some(at) = name
            .iter()
            .position(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_'))
        {
            return Err(TagError::Char { at, byte: name[at] });
        }
        let mut bytes = [0u8; TAG_MAX];
        bytes[..name.len()].copy_from_slice(name);
        Ok(Self {
            bytes,
            len: name.len() as u8,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

// ---------------------------------------------------------------------------
// Slot header
// ---------------------------------------------------------------------------

pub const HEADER_MAGIC: [u8; 4] = *b"ASPX";
pub const HEADER_VERSION: u16 = 1;
/// Encoded header size; see the table at the top of this module.
pub const HEADER_LEN: usize = 30;

const MAGIC_AT: usize = 0;
const VERSION_AT: usize = MAGIC_AT + HEADER_MAGIC.len();
const NAME_AT: usize = VERSION_AT + 2;
const PCM_BYTES_AT: usize = NAME_AT + TAG_MAX;
const PCM_CRC_AT: usize = PCM_BYTES_AT + 4;
const HEADER_CRC_AT: usize = PCM_CRC_AT + 2;
const _: () = assert!(HEADER_CRC_AT + 2 == HEADER_LEN);
const _: () = assert!(HEADER_LEN as u32 <= PCM_OFFSET);

/// Why bytes read from a slot base are not a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    /// Fewer than [`HEADER_LEN`] bytes supplied.
    Short {
        len: usize,
    },
    /// Not `ASPX` - including blank flash.
    Magic,
    Version {
        version: u16,
    },
    /// The header's own CRC does not match its contents.
    Crc {
        stored: u16,
        computed: u16,
    },
    Name(TagError),
    /// The name field has a non-NUL byte after its first NUL.
    NamePadding,
    Length(LengthError),
}

/// What a slot says it holds. Constructed only through [`SlotHeader::new`] or
/// [`SlotHeader::decode`], so an instance always encodes to a header that validates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHeader {
    name: Tag,
    pcm_bytes: u32,
    pcm_crc16: u16,
}

impl SlotHeader {
    pub fn new(name: Tag, pcm_bytes: u32, pcm_crc16: u16) -> Result<Self, LengthError> {
        check_pcm_len(pcm_bytes)?;
        Ok(Self {
            name,
            pcm_bytes,
            pcm_crc16,
        })
    }

    pub fn name(&self) -> &Tag {
        &self.name
    }

    pub fn pcm_bytes(&self) -> u32 {
        self.pcm_bytes
    }

    pub fn pcm_crc16(&self) -> u16 {
        self.pcm_crc16
    }

    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[MAGIC_AT..VERSION_AT].copy_from_slice(&HEADER_MAGIC);
        out[VERSION_AT..NAME_AT].copy_from_slice(&HEADER_VERSION.to_le_bytes());
        out[NAME_AT..PCM_BYTES_AT].copy_from_slice(&self.name.bytes);
        out[PCM_BYTES_AT..PCM_CRC_AT].copy_from_slice(&self.pcm_bytes.to_le_bytes());
        out[PCM_CRC_AT..HEADER_CRC_AT].copy_from_slice(&self.pcm_crc16.to_le_bytes());
        let crc = crc16_ccitt(&out[..HEADER_CRC_AT]);
        out[HEADER_CRC_AT..].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// Validate the first [`HEADER_LEN`] bytes of `raw`. Checks run magic, version, CRC, then
    /// fields, so blank flash reports [`HeaderError::Magic`] and a torn write reports
    /// [`HeaderError::Crc`] rather than whatever garbage field it happened to break first.
    pub fn decode(raw: &[u8]) -> Result<Self, HeaderError> {
        let Some(raw) = raw.get(..HEADER_LEN) else {
            return Err(HeaderError::Short { len: raw.len() });
        };
        if raw[MAGIC_AT..VERSION_AT] != HEADER_MAGIC {
            return Err(HeaderError::Magic);
        }
        let version = u16::from_le_bytes([raw[VERSION_AT], raw[VERSION_AT + 1]]);
        if version != HEADER_VERSION {
            return Err(HeaderError::Version { version });
        }
        let stored = u16::from_le_bytes([raw[HEADER_CRC_AT], raw[HEADER_CRC_AT + 1]]);
        let computed = crc16_ccitt(&raw[..HEADER_CRC_AT]);
        if stored != computed {
            return Err(HeaderError::Crc { stored, computed });
        }

        let field = &raw[NAME_AT..PCM_BYTES_AT];
        let len = field.iter().position(|b| *b == 0).unwrap_or(TAG_MAX);
        if field[len..].iter().any(|b| *b != 0) {
            return Err(HeaderError::NamePadding);
        }
        let name = Tag::new(&field[..len]).map_err(HeaderError::Name)?;

        let pcm_bytes = u32::from_le_bytes([
            raw[PCM_BYTES_AT],
            raw[PCM_BYTES_AT + 1],
            raw[PCM_BYTES_AT + 2],
            raw[PCM_BYTES_AT + 3],
        ]);
        let pcm_crc16 = u16::from_le_bytes([raw[PCM_CRC_AT], raw[PCM_CRC_AT + 1]]);
        Self::new(name, pcm_bytes, pcm_crc16).map_err(HeaderError::Length)
    }
}

// ---------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------

/// The only sample rate an excerpt may have: the codec's.
pub const WAV_SAMPLE_RATE: u32 = 48_000;

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

// ---------------------------------------------------------------------------
// Record grammar
// ---------------------------------------------------------------------------

const START_PREFIX: &[u8] = b"EXCSTART slot=";
const SEP_NAME: &[u8] = b" name=";
const SEP_BYTES: &[u8] = b" bytes=";
const SEP_CRC16: &[u8] = b" crc16=";
const DATA_PREFIX: &[u8] = b"EXCDATA i=";
const SEP_B64: &[u8] = b" b64=";
const END_BODY: &[u8] = b"EXCEND";

const SLOT_HEX_DIGITS: usize = 2;
const BYTES_DEC_DIGITS: usize = 6;
const CRC_HEX_DIGITS: usize = 4;
const INDEX_HEX_DIGITS: usize = 4;

/// Raw PCM bytes per `EXCDATA` record. See the module docs for why 128.
pub const EXC_CHUNK_RAW: usize = 128;

/// `EXCDATA` records in one sector.
pub const CHUNKS_PER_SECTOR: usize = SECTOR as usize / EXC_CHUNK_RAW;

/// Most `EXCDATA` records one install can need: 4,032.
pub const MAX_DATA_CHUNKS: usize = PCM_CAPACITY as usize / EXC_CHUNK_RAW;

/// Longest `EXCSTART` body: 62.
pub const MAX_START_BODY_LEN: usize = START_PREFIX.len()
    + SLOT_HEX_DIGITS
    + SEP_NAME.len()
    + TAG_MAX
    + SEP_BYTES.len()
    + BYTES_DEC_DIGITS
    + SEP_CRC16.len()
    + CRC_HEX_DIGITS;

/// Fixed part of an `EXCDATA` body, before the payload: 19.
pub const DATA_HEADER_LEN: usize = DATA_PREFIX.len() + INDEX_HEX_DIGITS + SEP_B64.len();

/// Longest `EXCDATA` body: 191.
pub const MAX_DATA_BODY_LEN: usize = DATA_HEADER_LEN + encoded_len(EXC_CHUNK_RAW);

const _: () = assert!((SECTOR as usize).is_multiple_of(EXC_CHUNK_RAW));
const _: () = assert!(EXC_CHUNK_RAW.is_multiple_of(2));
const _: () = assert!((PCM_CAPACITY as usize).is_multiple_of(EXC_CHUNK_RAW));
const _: () = assert!(CHUNKS_PER_SECTOR == 32);
const _: () = assert!(MAX_DATA_CHUNKS == 4032);
const _: () = assert!(MAX_DATA_CHUNKS <= 1 << (INDEX_HEX_DIGITS * 4));
const _: () = assert!((SLOT_COUNT as usize) <= 1 << (SLOT_HEX_DIGITS * 4));
const _: () = assert!(MAX_START_BODY_LEN == 62);
const _: () = assert!(MAX_DATA_BODY_LEN == 191);
const _: () = assert!(MAX_START_BODY_LEN <= MAX_BODY);
const _: () = assert!(MAX_DATA_BODY_LEN <= MAX_BODY);
const _: () = {
    check_hex_width(SLOT_HEX_DIGITS);
    check_hex_width(CRC_HEX_DIGITS);
    check_hex_width(INDEX_HEX_DIGITS);
    check_decimal_fits(PCM_CAPACITY, BYTES_DEC_DIGITS);
};

/// One `EXCDATA` payload, decoded: 1 to [`EXC_CHUNK_RAW`] bytes held inline so a parsed record
/// needs no allocation and borrows nothing from the frame buffer it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    bytes: [u8; EXC_CHUNK_RAW],
    len: u8,
}

impl Chunk {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// A well-formed, in-range excerpt record. Sequencing is not checked here; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcRecord {
    Start {
        slot: u8,
        name: Tag,
        bytes: u32,
        crc16: u16,
    },
    Data {
        index: u16,
        chunk: Chunk,
    },
    End,
}

/// Why a record body could not be built or parsed. Builders return only the range variants;
/// parsers return any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordError {
    /// The body is not an excerpt record at all.
    NotExcerpt,
    /// The body broke the grammar at byte `at`.
    Malformed {
        at: usize,
    },
    SlotOutOfRange {
        slot: u8,
    },
    Name(TagError),
    Length(LengthError),
    /// `index` is not below [`MAX_DATA_CHUNKS`].
    IndexOutOfRange {
        index: u16,
    },
    EmptyChunk,
    /// More than [`EXC_CHUNK_RAW`] bytes.
    ChunkTooLong {
        len: usize,
    },
    Payload(DecodeError),
}

/// Write `EXCSTART slot=.. name=.. bytes=.. crc16=..` into `out`, returning its length.
/// On refusal `out` is untouched.
pub fn exc_start_body(
    slot: u8,
    name: &Tag,
    bytes: u32,
    crc16: u16,
    out: &mut [u8; MAX_BODY],
) -> Result<usize, RecordError> {
    if slot >= SLOT_COUNT {
        return Err(RecordError::SlotOutOfRange { slot });
    }
    check_pcm_len(bytes).map_err(RecordError::Length)?;
    let dst = &mut out[..];
    let mut at = put(dst, 0, START_PREFIX);
    at = put_hex(dst, at, u32::from(slot), SLOT_HEX_DIGITS);
    at = put(dst, at, SEP_NAME);
    at = put(dst, at, name.as_bytes());
    at = put(dst, at, SEP_BYTES);
    at = put_decimal(dst, at, bytes, BYTES_DEC_DIGITS);
    at = put(dst, at, SEP_CRC16);
    at = put_hex(dst, at, u32::from(crc16), CRC_HEX_DIGITS);
    Ok(at)
}

/// Write `EXCDATA i=.. b64=..` carrying `raw` into `out`, returning its length.
/// On refusal `out` is untouched.
pub fn exc_data_body(
    index: u16,
    raw: &[u8],
    out: &mut [u8; MAX_BODY],
) -> Result<usize, RecordError> {
    if usize::from(index) >= MAX_DATA_CHUNKS {
        return Err(RecordError::IndexOutOfRange { index });
    }
    if raw.is_empty() {
        return Err(RecordError::EmptyChunk);
    }
    if raw.len() > EXC_CHUNK_RAW {
        return Err(RecordError::ChunkTooLong { len: raw.len() });
    }
    let dst = &mut out[..];
    let mut at = put(dst, 0, DATA_PREFIX);
    at = put_hex(dst, at, u32::from(index), INDEX_HEX_DIGITS);
    at = put(dst, at, SEP_B64);
    // Cannot fail: the length checks above bound the encoding inside MAX_DATA_BODY_LEN.
    let written =
        encode(raw, &mut dst[at..]).map_err(|_| RecordError::ChunkTooLong { len: raw.len() })?;
    Ok(at + written)
}

/// Write `EXCEND` into `out`, returning its length.
pub fn exc_end_body(out: &mut [u8; MAX_BODY]) -> usize {
    put(&mut out[..], 0, END_BODY)
}

/// Parse one frame body as an excerpt record.
///
/// Strict: fixed-width fields must be full width and lowercase, nothing may follow the last
/// field, values must be in range, and the payload must be canonical base64. Any `<`, `>`, `~` or
/// stray space therefore fails here, because no field's alphabet contains them.
pub fn parse_record(body: &[u8]) -> Result<ExcRecord, RecordError> {
    if body.starts_with(START_PREFIX) {
        parse_start(body)
    } else if body.starts_with(DATA_PREFIX) {
        parse_data(body)
    } else if body.starts_with(END_BODY) {
        if body.len() == END_BODY.len() {
            Ok(ExcRecord::End)
        } else {
            Err(RecordError::Malformed { at: END_BODY.len() })
        }
    } else {
        Err(RecordError::NotExcerpt)
    }
}

fn parse_start(body: &[u8]) -> Result<ExcRecord, RecordError> {
    let mut reader = BodyReader::new(body);
    let malformed = |reader: &BodyReader<'_>| RecordError::Malformed { at: reader.at };
    reader.literal(START_PREFIX).ok_or(malformed(&reader))?;
    let slot = reader.hex(SLOT_HEX_DIGITS).ok_or(malformed(&reader))? as u8;
    reader.literal(SEP_NAME).ok_or(malformed(&reader))?;
    let rest = reader.rest();
    let name_len = rest.iter().position(|b| *b == b' ').unwrap_or(rest.len());
    let name = Tag::new(&rest[..name_len]).map_err(RecordError::Name)?;
    reader.at += name_len;
    reader.literal(SEP_BYTES).ok_or(malformed(&reader))?;
    let bytes = reader.decimal(BYTES_DEC_DIGITS).ok_or(malformed(&reader))?;
    reader.literal(SEP_CRC16).ok_or(malformed(&reader))?;
    let crc16 = reader.hex(CRC_HEX_DIGITS).ok_or(malformed(&reader))? as u16;
    if !reader.rest().is_empty() {
        return Err(malformed(&reader));
    }
    if slot >= SLOT_COUNT {
        return Err(RecordError::SlotOutOfRange { slot });
    }
    check_pcm_len(bytes).map_err(RecordError::Length)?;
    Ok(ExcRecord::Start {
        slot,
        name,
        bytes,
        crc16,
    })
}

fn parse_data(body: &[u8]) -> Result<ExcRecord, RecordError> {
    let mut reader = BodyReader::new(body);
    let malformed = |reader: &BodyReader<'_>| RecordError::Malformed { at: reader.at };
    reader.literal(DATA_PREFIX).ok_or(malformed(&reader))?;
    let index = reader.hex(INDEX_HEX_DIGITS).ok_or(malformed(&reader))? as u16;
    reader.literal(SEP_B64).ok_or(malformed(&reader))?;
    if usize::from(index) >= MAX_DATA_CHUNKS {
        return Err(RecordError::IndexOutOfRange { index });
    }
    let mut chunk = Chunk {
        bytes: [0u8; EXC_CHUNK_RAW],
        len: 0,
    };
    let len = decode(reader.rest(), &mut chunk.bytes).map_err(|err| match err {
        DecodeError::OutputTooSmall { need } => RecordError::ChunkTooLong { len: need },
        other => RecordError::Payload(other),
    })?;
    if len == 0 {
        return Err(RecordError::EmptyChunk);
    }
    chunk.len = len as u8;
    Ok(ExcRecord::Data { index, chunk })
}
