//! Installing an excerpt: the host turns a PCM buffer into framed records, the device turns those
//! records back into whole flash sectors, and a readback decides whether the slot is trusted.
//!
//! Both halves live here so the wire cannot drift between them. [`install_frames`] is the only
//! code that emits an install stream - `examples/excerpt_stream.rs` wraps it, the tests drive it -
//! and [`Installer`] is the only code that consumes one.
//!
//! # The device half is a state machine with no I/O
//!
//! [`Installer::feed`] takes one parsed record and answers with one [`Action`]. The caller does
//! the flash work the action names and then feeds the next record; the sector a
//! [`Action::WriteSector`] lends out borrows the installer, so the borrow checker refuses a feed
//! while a sector is still being written. That is what lets the installer hold exactly one 4 KiB
//! buffer and never a queue.
//!
//! The slot is never trusted part-way:
//!
//! 1. `EXCSTART` answers [`Action::InvalidateHeader`]. The caller erases the header sector first,
//!    so a slot whose install is interrupted anywhere after this reads as blank flash
//!    ([`super::HeaderError::Magic`]) rather than as the excerpt it used to hold.
//! 2. Each `EXCDATA` lands in the sector buffer. When the buffer fills - or the last declared byte
//!    arrives, in which case the tail is padded with `0xFF` - the answer is
//!    [`Action::WriteSector`]. [`super::EXC_CHUNK_RAW`] divides [`super::SECTOR`], so a chunk
//!    never straddles two sectors and one record never needs two writes.
//! 3. `EXCEND` answers [`Action::Verify`]. The caller reads the PCM back from flash and passes its
//!    CRC and length to [`Installer::verdict`].
//! 4. Only [`Verdict::Ok`] carries the encoded [`super::SlotHeader`]. Writing it is the last flash
//!    operation of an install, so the header's existence is the proof that every byte before it
//!    was read back and matched.
//!
//! The installer also keeps a running CRC over the bytes it accepted and refuses an `EXCEND`
//! whose stream disagrees with the declared `crc16`. That catches a host that declared the wrong
//! CRC before any flash is trusted, but it is not the authoritative check: only the readback sees
//! what the flash actually holds.
//!
//! # Strictness
//!
//! Every deviation fails the install, and a failed install cannot reach [`Verdict::Ok`]:
//!
//! | input | why tag |
//! |---|---|
//! | `EXCDATA i` other than the next expected index (gap, duplicate, reorder) | `order` |
//! | a chunk shorter than [`super::EXC_CHUNK_RAW`] that does not end the excerpt (this covers odd lengths) | `short` |
//! | a chunk that carries bytes past the declared length | `overrun` |
//! | `EXCEND` before every declared byte arrived | `early` |
//! | `EXCDATA` or `EXCEND` with no install open | `nostart` |
//! | a record the grammar refuses, including an `EXCSTART` with an odd or over-capacity length | `malformed` |
//! | streamed bytes whose CRC is not the declared one | `stream` |
//! | [`Installer::verdict`] called with no install awaiting one | `state` |
//! | readback length differs from the declared one | `length` |
//! | readback CRC differs from the declared one | (none: the bare `got=`/`want=` form) |
//!
//! A frame corrupted on the wire never reaches the installer - [`crate::frame::Decoder`] drops it
//! on its own CRC - so corruption shows up here as a gap, which `order` refuses.
//!
//! Failure is sticky: after a [`Action::Reject`], every `EXCDATA` and `EXCEND` of the dead install
//! is [`Action::Ignored`] (one rejection per install, not one per remaining chunk) until the next
//! `EXCSTART`. An `EXCSTART` in any state begins a fresh install and abandons whatever was open;
//! the abandoned slot's header was already invalidated by its own `EXCSTART`, so it reads blank.

use log::Level;

use super::{
    exc_data_body, exc_end_body, exc_start_body, parse_record, slot_base, slot_pcm_base, ExcRecord,
    RecordError, SlotHeader, Tag, EXC_CHUNK_RAW, HEADER_LEN, SECTOR,
};
use crate::dump::{put, put_decimal, put_hex};
use crate::frame::{crc16_ccitt, crc16_ccitt_update, encode, CRC16_INITIAL, MAX_BODY, MAX_FRAME};

const SECTOR_BYTES: usize = SECTOR as usize;

// ---------------------------------------------------------------------------
// Host side: the frame stream
// ---------------------------------------------------------------------------

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
        .map_err(|_| RecordError::Length(super::LengthError::OverCapacity { bytes: u32::MAX }))?;
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

// ---------------------------------------------------------------------------
// Device side: the installer
// ---------------------------------------------------------------------------

/// Why an install failed. Rendered by [`exc_fail_body`]; see the table in the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Readback CRC differs from the declared one.
    Crc,
    /// Readback length differs from the declared one.
    Length,
    /// The streamed bytes' CRC differs from the declared one.
    Stream,
    /// `EXCDATA` or `EXCEND` with no install open.
    NoStart,
    /// `EXCDATA` index was not the next expected one.
    Order { expected: u16, got: u16 },
    /// A chunk shorter than [`EXC_CHUNK_RAW`] that does not finish the excerpt.
    Short { index: u16, len: u8 },
    /// A chunk that runs past the declared length.
    Overrun { index: u16 },
    /// `EXCEND` with `got` of `declared` bytes received.
    Early { got: u32, declared: u32 },
    /// The record body failed [`parse_record`].
    Malformed(RecordError),
    /// [`Installer::verdict`] with no install awaiting verification.
    State,
}

impl Why {
    /// The `why=` tag, or `None` for a plain CRC mismatch.
    pub fn tag(&self) -> Option<&'static str> {
        match self {
            Why::Crc => None,
            Why::Length => Some("length"),
            Why::Stream => Some("stream"),
            Why::NoStart => Some("nostart"),
            Why::Order { .. } => Some("order"),
            Why::Short { .. } => Some("short"),
            Why::Overrun { .. } => Some("overrun"),
            Why::Early { .. } => Some("early"),
            Why::Malformed(_) => Some("malformed"),
            Why::State => Some("state"),
        }
    }
}

/// A failed install: what the device has (`got`), what the host declared (`want`), and why.
///
/// `got` is the readback CRC for a verdict, and the running CRC of the bytes accepted so far for a
/// rejection mid-stream. `want` is the declared `crc16`, or 0 when no install was open to declare
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub got: u16,
    pub want: u16,
    pub why: Why,
}

/// What the caller must do after [`Installer::feed`].
#[derive(Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// An install began. Erase the sector at `address` (the slot's header sector) before anything
    /// else, so the slot reads blank until the install verifies. `aborted` names the slot of an
    /// install this one replaced, if any.
    InvalidateHeader {
        slot: u8,
        address: u32,
        aborted: Option<u8>,
    },
    /// Write `data` at `address`. `sector` counts from the slot base, so the first PCM sector is
    /// 1 (sector 0 is the header's). The borrow ends before the next feed, which is the point.
    WriteSector {
        sector: u16,
        address: u32,
        data: &'a [u8; SECTOR_BYTES],
    },
    /// The chunk was accepted into the sector buffer; nothing to write yet.
    Accepted,
    /// Every byte was written. Read `bytes` bytes back from `address`, CRC them, and pass the
    /// result to [`Installer::verdict`].
    Verify { slot: u8, address: u32, bytes: u32 },
    /// The install failed. Report it with [`exc_fail_body`]; the slot's header stays invalid.
    Reject(Failure),
    /// Part of an install that already failed, or a record that is not an excerpt record at all.
    Ignored,
}

/// What [`Installer::verdict`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The readback matched. Write `header` at `address` (the slot base) as the final flash
    /// operation of the install, then report [`exc_ok_body`].
    Ok {
        slot: u8,
        address: u32,
        header: SlotHeader,
        encoded: [u8; HEADER_LEN],
    },
    Fail(Failure),
}

#[derive(Debug, Clone, Copy)]
struct Open {
    slot: u8,
    name: Tag,
    declared: u32,
    want: u16,
    got: u32,
    crc: u16,
    next_index: u16,
    fill: usize,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Receiving(Open),
    Verifying(Open),
    Failed,
}

/// The device half of an install. See the module docs for the protocol it enforces.
///
/// 4 KiB of sector buffer plus a few words of state, so it belongs in a static or a task's
/// future, not on a small stack.
pub struct Installer {
    state: State,
    sector: [u8; SECTOR_BYTES],
}

impl Default for Installer {
    fn default() -> Self {
        Self::new()
    }
}

impl Installer {
    pub const fn new() -> Self {
        Self {
            state: State::Idle,
            sector: [0xFF; SECTOR_BYTES],
        }
    }

    /// Parse a frame body and feed it. A body that is not an excerpt record at all is
    /// [`Action::Ignored`] - it belongs to some other consumer - while one that claims to be an
    /// excerpt record and breaks the grammar fails the open install.
    pub fn feed_body(&mut self, body: &[u8]) -> Action<'_> {
        match parse_record(body) {
            Ok(record) => self.feed(record),
            Err(RecordError::NotExcerpt) => Action::Ignored,
            Err(err) => self.fail(Why::Malformed(err)),
        }
    }

    /// Advance the install by one record.
    pub fn feed(&mut self, record: ExcRecord) -> Action<'_> {
        match record {
            ExcRecord::Start {
                slot,
                name,
                bytes,
                crc16,
            } => {
                let aborted = match self.state {
                    State::Receiving(open) | State::Verifying(open) => Some(open.slot),
                    State::Idle | State::Failed => None,
                };
                self.state = State::Receiving(Open {
                    slot,
                    name,
                    declared: bytes,
                    want: crc16,
                    got: 0,
                    crc: CRC16_INITIAL,
                    next_index: 0,
                    fill: 0,
                });
                Action::InvalidateHeader {
                    slot,
                    // Cannot be None: parse_record and exc_start_body both refuse slot >= SLOT_COUNT.
                    address: slot_base(slot).unwrap_or(0),
                    aborted,
                }
            }
            ExcRecord::Data { index, chunk } => {
                let mut open = match self.state {
                    State::Receiving(open) => open,
                    State::Failed => return Action::Ignored,
                    State::Idle | State::Verifying(_) => return self.fail(Why::NoStart),
                };
                if index != open.next_index {
                    return self.fail(Why::Order {
                        expected: open.next_index,
                        got: index,
                    });
                }
                let raw = chunk.as_bytes();
                let len = raw.len() as u32;
                let remaining = open.declared - open.got;
                if len > remaining {
                    return self.fail(Why::Overrun { index });
                }
                if raw.len() < EXC_CHUNK_RAW && len != remaining {
                    return self.fail(Why::Short {
                        index,
                        len: raw.len() as u8,
                    });
                }

                // EXC_CHUNK_RAW divides SECTOR and every chunk before the last is full, so `fill`
                // is a multiple of EXC_CHUNK_RAW here and the chunk fits.
                self.sector[open.fill..open.fill + raw.len()].copy_from_slice(raw);
                open.fill += raw.len();
                open.got += len;
                open.crc = crc16_ccitt_update(open.crc, raw);
                open.next_index = open.next_index.wrapping_add(1);

                let complete = open.got == open.declared;
                if open.fill < SECTOR_BYTES && !complete {
                    self.state = State::Receiving(open);
                    return Action::Accepted;
                }
                self.sector[open.fill..].fill(0xFF);
                let sector = open.got.div_ceil(SECTOR) as u16;
                let address = slot_base(open.slot).unwrap_or(0) + u32::from(sector) * SECTOR;
                open.fill = 0;
                self.state = State::Receiving(open);
                Action::WriteSector {
                    sector,
                    address,
                    data: &self.sector,
                }
            }
            ExcRecord::End => {
                let open = match self.state {
                    State::Receiving(open) => open,
                    State::Failed => {
                        self.state = State::Idle;
                        return Action::Ignored;
                    }
                    State::Idle => return self.fail(Why::NoStart),
                    State::Verifying(_) => return self.fail(Why::NoStart),
                };
                if open.got != open.declared {
                    return self.fail(Why::Early {
                        got: open.got,
                        declared: open.declared,
                    });
                }
                if open.crc != open.want {
                    return self.fail(Why::Stream);
                }
                self.state = State::Verifying(open);
                Action::Verify {
                    slot: open.slot,
                    address: slot_pcm_base(open.slot).unwrap_or(0),
                    bytes: open.declared,
                }
            }
        }
    }

    /// Decide the install from the readback after [`Action::Verify`]: `got_crc` is the
    /// CRC-16/CCITT-FALSE of the `got_len` bytes read back from flash. The installer returns to
    /// idle either way.
    pub fn verdict(&mut self, got_crc: u16, got_len: u32) -> Verdict {
        let open = match self.state {
            State::Verifying(open) => open,
            State::Receiving(open) => {
                self.state = State::Failed;
                return Verdict::Fail(Failure {
                    got: got_crc,
                    want: open.want,
                    why: Why::State,
                });
            }
            State::Idle | State::Failed => {
                return Verdict::Fail(Failure {
                    got: got_crc,
                    want: 0,
                    why: Why::State,
                })
            }
        };
        self.state = State::Idle;
        let failure = |why| {
            Verdict::Fail(Failure {
                got: got_crc,
                want: open.want,
                why,
            })
        };
        if got_len != open.declared {
            return failure(Why::Length);
        }
        if got_crc != open.want {
            return failure(Why::Crc);
        }
        match SlotHeader::new(open.name, open.declared, open.want) {
            Ok(header) => Verdict::Ok {
                slot: open.slot,
                address: slot_base(open.slot).unwrap_or(0),
                header,
                encoded: header.encode(),
            },
            // Unreachable: `declared` passed check_pcm_len when EXCSTART parsed.
            Err(_) => failure(Why::Length),
        }
    }

    /// Whether an install is open (receiving or awaiting its verdict).
    pub fn is_open(&self) -> bool {
        matches!(self.state, State::Receiving(_) | State::Verifying(_))
    }

    fn fail(&mut self, why: Why) -> Action<'_> {
        let (got, want) = match self.state {
            State::Receiving(open) | State::Verifying(open) => (open.crc, open.want),
            State::Idle | State::Failed => (0, 0),
        };
        self.state = State::Failed;
        Action::Reject(Failure { got, want, why })
    }
}

// ---------------------------------------------------------------------------
// Verdict records
// ---------------------------------------------------------------------------

const OK_PREFIX: &[u8] = b"EXCOK name=";
const SEP_BYTES: &[u8] = b" bytes=";
const SEP_CRC16: &[u8] = b" crc16=";
const FAIL_PREFIX: &[u8] = b"EXCFAIL got=";
const SEP_WANT: &[u8] = b" want=";
const SEP_WHY: &[u8] = b" why=";
const LONGEST_WHY: usize = 9; // "malformed"

/// Longest `EXCOK` body: 51.
pub const MAX_OK_BODY_LEN: usize =
    OK_PREFIX.len() + super::TAG_MAX + SEP_BYTES.len() + 6 + SEP_CRC16.len() + 4;

/// Longest `EXCFAIL` body: 40.
pub const MAX_FAIL_BODY_LEN: usize =
    FAIL_PREFIX.len() + 4 + SEP_WANT.len() + 4 + SEP_WHY.len() + LONGEST_WHY;

const _: () = assert!(MAX_OK_BODY_LEN == 51);
const _: () = assert!(MAX_FAIL_BODY_LEN == 40);
const _: () = assert!(MAX_OK_BODY_LEN <= MAX_BODY);
const _: () = assert!(MAX_FAIL_BODY_LEN <= MAX_BODY);

/// Write `EXCOK name=<tag> bytes=<6 dec> crc16=<4 hex>` into `out`, returning its length.
pub fn exc_ok_body(header: &SlotHeader, out: &mut [u8; MAX_BODY]) -> usize {
    let dst = &mut out[..];
    let mut at = put(dst, 0, OK_PREFIX);
    at = put(dst, at, header.name().as_bytes());
    at = put(dst, at, SEP_BYTES);
    at = put_decimal(dst, at, header.pcm_bytes(), 6);
    at = put(dst, at, SEP_CRC16);
    put_hex(dst, at, u32::from(header.pcm_crc16()), 4)
}

/// Write `EXCFAIL got=<4 hex> want=<4 hex>`, plus ` why=<tag>` for anything but a plain CRC
/// mismatch, into `out`, returning its length.
pub fn exc_fail_body(failure: &Failure, out: &mut [u8; MAX_BODY]) -> usize {
    let dst = &mut out[..];
    let mut at = put(dst, 0, FAIL_PREFIX);
    at = put_hex(dst, at, u32::from(failure.got), 4);
    at = put(dst, at, SEP_WANT);
    at = put_hex(dst, at, u32::from(failure.want), 4);
    if let Some(tag) = failure.why.tag() {
        at = put(dst, at, SEP_WHY);
        at = put(dst, at, tag.as_bytes());
    }
    at
}
