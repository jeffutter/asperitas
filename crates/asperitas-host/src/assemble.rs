//! The receiving half of the audio-dump grammar: [`BlockAssembler`] consumes `AUDIO` and `AUDEND`
//! record bodies and reassembles whole blocks of PCM, refusing any block whose completeness it
//! cannot prove from `n_of_n` sequence plus a block checksum.
//!
//! The grammar, its constants and the encoder live in [`asperitas_wire::dump`]; this module reads
//! the same templates and field widths from there, so a change to the writer moves the parser with
//! it. The format only earns the word "self-verifying" because something acts on that proof, and
//! this is that something.

use asperitas_wire::dump::{
    decode, max_raw_for, BodyReader, DecodeError, AUDEND_PREFIX, AUDIO_HEADER_LEN, AUDIO_PREFIX,
    BLK_HEX_DIGITS, BYTES_DEC_DIGITS, CHUNK_RAW, COUNT_FIELD_MODULUS, COUNT_HEX_DIGITS,
    CRC_HEX_DIGITS, MAX_AUDEND_BODY_LEN, SEP_BYTES, SEP_C, SEP_CRC16, SEP_D, SEP_N,
};
use asperitas_wire::frame::crc16_ccitt;

/// Bits per presence-mask word.
const MASK_BITS: usize = 64;

/// Words needed for one bit per chunk index the `n` field can name. The modulus is used rather
/// than [`MAX_CHUNKS_PER_BLOCK`] so index 255 lands inside a word without a bounds check anywhere
/// else, and so widening the field widens the mask with it.
const MASK_WORDS: usize = COUNT_FIELD_MODULUS.div_ceil(MASK_BITS);

/// Indices a printed missing-list names before ellipsising. A block missing all 255 chunks is one
/// lost span of wire, not 255 facts worth scrolling past.
const DEBUG_MISSING_INDICES: usize = 12;

/// Which chunks a refused block never received.
///
/// A refusal is only actionable if it names what is absent: “block 3 failed” tells an operator
/// nothing, “block 3 is missing chunks 4 and 5 of 6” says a specific run of records went away. The
/// set is a fixed bitmask rather than a list, so it stays `Copy` and a block missing every chunk
/// costs the same 32 bytes as one missing a single chunk.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MissingChunks {
    /// Bit `i` set ⇒ chunk index `i` never arrived.
    words: [u64; MASK_WORDS],
    count: usize,
}

impl MissingChunks {
    /// Nothing missing — the mask a caller starts from before recording anything absent.
    const NONE: Self = Self {
        words: [0; MASK_WORDS],
        count: 0,
    };

    fn insert(&mut self, index: u16) {
        self.words[(index as usize) / MASK_BITS] |= 1 << ((index as usize) % MASK_BITS);
        self.count += 1;
    }

    /// How many chunks the refused block never received.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Whether chunk `index` is among those never received. An index the grammar cannot name is
    /// simply not missing, which keeps this total for any `u16` instead of panicking on one.
    pub fn contains(&self, index: u16) -> bool {
        let slot = index as usize;
        slot < COUNT_FIELD_MODULUS && (self.words[slot / MASK_BITS] >> (slot % MASK_BITS)) & 1 == 1
    }

    /// Lowest missing index — the one a one-line diagnostic should name.
    pub fn first(&self) -> Option<u16> {
        self.after(0)
    }

    /// Lowest missing index at or after `from`. Linear over a field the grammar caps at 256 values:
    /// bit-scanning arithmetic here would be less code nobody reads, not measurably more speed.
    fn after(&self, from: u16) -> Option<u16> {
        (from..COUNT_FIELD_MODULUS as u16).find(|index| self.contains(*index))
    }
}

/// Print the indices themselves: these values land in test failures and capture summaries, where
/// twenty-five lines of bitfield internals describe nothing.
impl core::fmt::Debug for MissingChunks {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} missing [", self.count)?;
        let mut shown = 0usize;
        let mut separator = "";
        let mut cursor = 0u16;
        while shown < DEBUG_MISSING_INDICES {
            let Some(index) = self.after(cursor) else {
                break;
            };
            write!(f, "{separator}{index}")?;
            separator = " ";
            shown += 1;
            cursor = index + 1;
        }
        if shown < self.count {
            write!(f, "{separator}…")?;
        }
        write!(f, "]")
    }
}

/// One thing a consumed record did.
///
/// Every variant names its block, so a caller logging events never reconstructs which block an
/// event belonged to. There is deliberately no `Started` variant: a block announces itself by its
/// first stored chunk, and an event whose only content is “state changed” asks the caller to infer
/// the consequence from context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A chunk landed at `chunk · CHUNK_RAW` for the first time.
    ChunkStored {
        block: u32,
        chunk: u16,
        bytes: usize,
    },
    /// The same chunk index arrived again carrying byte-identical data: harmless, and expected
    /// whenever a writer retries a record it could not confirm had left the device.
    Duplicate { block: u32, chunk: u16 },
    /// The same chunk index arrived carrying *different* bytes. The earlier copy is kept — a
    /// re-send arriving after a partial write must not overwrite data that already validated
    /// against the block's own checksum — and the block is marked conflicted.
    Conflict { block: u32, chunk: u16 },
    /// Every chunk arrived, the byte counts agree with the geometry, and the block checksum matches
    /// the assembled bytes. [`BlockAssembler::pcm`] names them.
    Complete { block: u32, bytes: usize },
    /// This block will not yield samples. The reason names what to look for on the wire.
    Failed { block: u32, reason: Failure },
    /// A record for a block that has already closed: counted and dropped, because applying it would
    /// mean reopening a verdict this assembler already published.
    LateRecord { block: u32, chunk: Option<u16> },
    /// A record named a different block while one was open, so the open block's remaining chunks
    /// are gone for good. Carries what had not arrived when it died.
    Abandoned { block: u32, missing: MissingChunks },
    /// A body opened with a dump verb but does not have that verb's shape. `at` is the first byte
    /// that broke the grammar, counted from the start of the body.
    Malformed { at: usize },
    /// Not dump traffic at all — `BOOT`, `STATUS`, `PANIC`, a log line. Passing uncounted as loss is
    /// the point: this assembler sits on the receiving end of a console stream, not a dedicated
    /// channel that does not exist.
    Ignored,
}

/// Why a block was refused.
///
/// These are not severities. Each one points at something different to go looking for, and several
/// can only be told apart by the numbers they carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Chunks never arrived. Loss proven by sequence, which is the property no checksum supplies.
    Missing(MissingChunks),
    /// All chunks arrived and [`crc16_ccitt`] over the assembled bytes disagrees with what `AUDEND`
    /// shipped.
    Crc { shipped: u16, computed: u16 },
    /// Chunk `index` was sent twice with different bytes, so the block has two candidate shapes.
    Conflict { index: u16 },
    /// The caller's staging buffer cannot hold a chunk at its geometric offset; the block needs at
    /// least `needed` bytes.
    Capacity { needed: usize },
    /// The block's size does not add up. `placed` is what the chunks actually carried, `expected` is
    /// what the geometry requires — `(n − 1) · CHUNK_RAW` plus the final chunk — and `declared` is
    /// what `AUDEND` claimed. Comparing placed against expected is what proves every non-final chunk
    /// was full: placement puts chunk `i` at `i · CHUNK_RAW`, so a short chunk anywhere but last
    /// leaves a hole no honest total can account for.
    Length {
        declared: u32,
        placed: usize,
        expected: usize,
    },
    /// Chunks of one block disagree about how many chunks the block has, so its geometry has no
    /// single answer. Only corruption produces this: one writer emits one `n` per block.
    ChunkCount { expected: u16, found: u16 },
}

/// The one or two events a single consumed record produced, oldest first.
///
/// Two happen when a record names a block other than the one open: the open block loses its last
/// chance to complete *and* the arriving record gets its own verdict. Both are reported rather than
/// the more interesting one being chosen, because a tool whose entire job is proving absence may not
/// drop the fact that a block died. At most two can occur: a record names one block, so it can kill
/// at most one other block and decide at most itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Actions {
    /// Events in order. Slots past [`Actions::len`] hold [`Action::Ignored`] purely to keep the
    /// array initialised; `as_slice` never shows them.
    items: [Action; 2],
    len: usize,
}

impl Actions {
    /// Empty — the state `accept` starts from before the record has been classified.
    fn none() -> Self {
        Self {
            items: [Action::Ignored; 2],
            len: 0,
        }
    }

    fn push(&mut self, action: Action) {
        // Two is the provable maximum (see the type documentation); a third would mean the state
        // machine started reporting things it did not do, so debug builds stop rather than choose.
        debug_assert!(
            self.len < self.items.len(),
            "more than two events per record"
        );
        if self.len < self.items.len() {
            self.items[self.len] = action;
            self.len += 1;
        }
    }

    /// The events in order. Never empty in practice: every body is either a dump verb or
    /// [`Action::Ignored`].
    pub fn as_slice(&self) -> &[Action] {
        &self.items[..self.len]
    }

    /// The verdict on the record just consumed — the last event when a record also killed an older
    /// block, which is the overwhelmingly common single-event case too.
    pub fn last(&self) -> Option<Action> {
        self.as_slice().last().copied()
    }
}

/// Print the events as the list they are, not as an array padded with placeholders.
impl core::fmt::Debug for Actions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

/// What a [`BlockAssembler`] saw, in the categories a capture summary reports.
///
/// The assembler counts these itself rather than leaving callers to tally returned [`Action`]s: the
/// counting then has one owner, and a manifest cannot drift from the state machine that produced it.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Records consumed, dump traffic and log chatter alike.
    pub records: u64,
    /// Records that were neither `AUDIO` nor `AUDEND` — the console stream flowing past untouched.
    pub non_dump_records: u64,
    /// Dump-shaped bodies that did not parse. Never charged as loss on their own: a chunk that never
    /// got stored surfaces again as [`Failure::Missing`], which is the honest accounting.
    pub malformed_records: u64,
    pub chunks_stored: u64,
    pub duplicate_chunks: u64,
    pub conflicts: u64,
    pub late_records: u64,
    pub blocks_completed: u64,
    pub blocks_failed: u64,
    pub blocks_abandoned: u64,
    /// Raw PCM bytes handed out by completed blocks.
    pub pcm_bytes: u64,
}

impl Tally {
    /// Charge one event. One funnel, so a new variant cannot escape the counters by forgetting a
    /// call site.
    fn observe(&mut self, action: &Action) {
        match action {
            Action::ChunkStored { .. } => self.chunks_stored += 1,
            Action::Duplicate { .. } => self.duplicate_chunks += 1,
            Action::Conflict { .. } => self.conflicts += 1,
            Action::Complete { bytes, .. } => {
                self.blocks_completed += 1;
                self.pcm_bytes += *bytes as u64;
            }
            Action::Failed { .. } => self.blocks_failed += 1,
            Action::LateRecord { .. } => self.late_records += 1,
            Action::Abandoned { .. } => self.blocks_abandoned += 1,
            Action::Malformed { .. } => self.malformed_records += 1,
            Action::Ignored => self.non_dump_records += 1,
        }
    }
}

/// A block that died before its summary arrived, and what it still needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abandoned {
    pub block: u32,
    pub missing: MissingChunks,
}

/// What [`BlockAssembler::finish`] reported about a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finish {
    /// The block still open when the bytes ran out. Its remaining chunks may merely not have been
    /// sent yet, but nothing further can arrive, so it is indistinguishable from loss and refused as
    /// such.
    pub abandoned: Option<Abandoned>,
    pub tally: Tally,
}

/// A block whose chunks are arriving but whose summary has not.
#[derive(Clone, Copy)]
struct OpenBlock {
    block: u32,
    /// Chunk count as announced by this block's own chunks, not by a summary.
    chunks: u16,
    present: [u64; MASK_WORDS],
    /// Distinct indices stored.
    stored: usize,
    /// Raw bytes those indices carried, summed once per index.
    placed: usize,
    /// Raw length of the chunk at index `chunks - 1`; zero until it arrives.
    final_len: usize,
    /// First index seen twice with different bytes.
    conflict: Option<u16>,
}

impl OpenBlock {
    fn new(block: u32, chunks: u16) -> Self {
        Self {
            block,
            chunks,
            present: [0; MASK_WORDS],
            stored: 0,
            placed: 0,
            final_len: 0,
            conflict: None,
        }
    }

    fn is_present(&self, index: u16) -> bool {
        (self.present[(index as usize) / MASK_BITS] >> ((index as usize) % MASK_BITS)) & 1 == 1
    }

    fn mark_present(&mut self, index: u16) {
        self.present[(index as usize) / MASK_BITS] |= 1 << ((index as usize) % MASK_BITS);
        self.stored += 1;
    }

    fn complete(&self) -> bool {
        self.stored == self.chunks as usize
    }

    fn missing(&self) -> MissingChunks {
        let mut missing = MissingChunks::NONE;
        for index in 0..self.chunks {
            if !self.is_present(index) {
                missing.insert(index);
            }
        }
        missing
    }
}

/// Where each `AUDIO` field starts. Derived from the writer's own templates, so editing a template
/// moves the parser and its diagnostics with it, and the assertion below turns drift into a build
/// failure rather than a parser quietly reading the wrong bytes.
const CHUNK_BLK_AT: usize = AUDIO_PREFIX.len();
const CHUNK_COUNT_AT: usize = CHUNK_BLK_AT + BLK_HEX_DIGITS + SEP_N.len();
const CHUNK_INDEX_AT: usize = CHUNK_COUNT_AT + COUNT_HEX_DIGITS + SEP_C.len();
const CHUNK_PAYLOAD_AT: usize = CHUNK_INDEX_AT + COUNT_HEX_DIGITS + SEP_D.len();

/// Where each `AUDEND` field starts.
const SUMMARY_BLK_AT: usize = AUDEND_PREFIX.len();
const SUMMARY_COUNT_AT: usize = SUMMARY_BLK_AT + BLK_HEX_DIGITS + SEP_N.len();
const SUMMARY_BYTES_AT: usize = SUMMARY_COUNT_AT + COUNT_HEX_DIGITS + SEP_BYTES.len();
const SUMMARY_CRC_AT: usize = SUMMARY_BYTES_AT + BYTES_DEC_DIGITS + SEP_CRC16.len();

const _: () = assert!(CHUNK_PAYLOAD_AT == AUDIO_HEADER_LEN);
const _: () = assert!(SUMMARY_CRC_AT + CRC_HEX_DIGITS == MAX_AUDEND_BODY_LEN);

/// The fields of an `AUDIO` body, payload left encoded: only the assembler decodes it, and only into
/// a block's slot.
struct ChunkFields<'b> {
    block: u32,
    chunks: u16,
    index: u16,
    payload: &'b [u8],
}

/// The fields of an `AUDEND` body.
struct SummaryFields {
    block: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
}

/// Parse an `AUDIO` body; on failure, the body offset that broke the shape.
fn parse_chunk(body: &[u8]) -> Result<ChunkFields<'_>, usize> {
    let mut reader = BodyReader::new(body);
    reader.literal(AUDIO_PREFIX).ok_or(reader.at)?;
    let block = reader.hex(BLK_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_N).ok_or(reader.at)?;
    let chunks = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_C).ok_or(reader.at)?;
    let index = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_D).ok_or(reader.at)?;
    Ok(ChunkFields {
        block,
        // Both fields are two hex digits wide, so the values cannot exceed 255 and truncating to
        // `u16` drops nothing.
        chunks: chunks as u16,
        index: index as u16,
        payload: reader.rest(),
    })
}

/// Parse an `AUDEND` body; on failure, the body offset that broke the shape.
fn parse_summary(body: &[u8]) -> Result<SummaryFields, usize> {
    let mut reader = BodyReader::new(body);
    reader.literal(AUDEND_PREFIX).ok_or(reader.at)?;
    let block = reader.hex(BLK_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_N).ok_or(reader.at)?;
    let chunks = reader.hex(COUNT_HEX_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_BYTES).ok_or(reader.at)?;
    let total_bytes = reader.decimal(BYTES_DEC_DIGITS).ok_or(reader.at)?;
    reader.literal(SEP_CRC16).ok_or(reader.at)?;
    let crc = reader.hex(CRC_HEX_DIGITS).ok_or(reader.at)?;
    Ok(SummaryFields {
        block,
        chunks: chunks as u16,
        total_bytes,
        crc: crc as u16,
    })
}

/// Assemble `AUDIO`/`AUDEND` records into whole blocks of raw PCM, refusing every block whose
/// completeness it cannot prove.
///
/// Feed it [`asperitas_wire::frame::Record::body`] slices as the console decoder delivers them: it parses the
/// dump verbs itself and ignores everything else. All it needs from the caller is one staging buffer,
/// which it borrows for as long as it lives:
///
/// ```text
/// let mut staging = [0u8; 8192];
/// let mut assembler = BlockAssembler::new(&mut staging);
/// while let Some(record) = decoder.next_record() {
///     for action in assembler.accept(record.body).as_slice() {
///         if let Action::Complete { bytes, .. } = action {
///             file.write_all(assembler.pcm())?;   // exactly `bytes` long
///         }
///     }
/// }
/// let finish = assembler.finish();
/// ```
///
/// # Completeness is proved by sequence, never by checksum alone
///
/// A block completes when, and only when:
///
/// 1. every chunk index `0..n` arrived at least once,
/// 2. no index arrived twice with different bytes,
/// 3. the bytes the chunks carry add up both to what the geometry requires — `(n − 1) · CHUNK_RAW`
///    plus the final chunk — and to what `AUDEND` declared, and
/// 4. [`crc16_ccitt`] over the assembled bytes equals the value `AUDEND` shipped.
///
/// Condition 1 is the one no checksum supplies. [`asperitas_wire::frame`] documents that a record whose
/// leading `~` vanished produces no integrity failure at all, so a lost chunk looks exactly like a
/// quiet wire; `n_of_n` is what turns that silence into a refusal. In the other direction a CRC
/// cannot detect permutation, which is why chunks are *placed* at `chunk_index · CHUNK_RAW` rather
/// than appended: arrival order then cannot change the result structurally rather than by luck, and
/// `tests/console_dump.rs` shuffles streams to show it rather than assume it.
///
/// # One open block, because interleaving is loss rather than concurrency
///
/// A single producer writes a block's chunks in order, so a record naming a different block while
/// one is open means the open block's tail is gone. That block is reported [`Action::Abandoned`]
/// with its missing list and the new one takes its place. Holding several blocks open would require
/// deciding which gaps were real, and the answer would be a heuristic sitting underneath a tool whose
/// whole value is certainty.
///
/// Only the most recently closed block id is remembered, which is enough to classify the realistic
/// late arrival: a re-send of the block that just finished. A chunk from one further back opens a
/// fresh block and refuses whatever it displaced — still a refusal, never a false completion.
pub struct BlockAssembler<'a> {
    /// Assembled bytes for the block in play, indexed by chunk geometry.
    buf: &'a mut [u8],
    open: Option<OpenBlock>,
    /// Most recent block that reached a verdict, so a record arriving after it is recognised as late
    /// instead of resurrecting a published decision.
    last_closed: Option<u32>,
    tally: Tally,
    /// Length of the last completed block, marking the meaningful prefix of `buf`. Zero whenever a
    /// block other than a completed one is in play, so a caller cannot read out another block's
    /// leftovers by holding on to a stale view.
    completed: usize,
}

impl<'a> BlockAssembler<'a> {
    /// An assembler writing into `buf`, which must outlive the assembler.
    ///
    /// No minimum size is imposed: a buffer too small for a block is discovered when a chunk cannot
    /// be placed and reported as [`Failure::Capacity`], which beats a constructor that refuses to
    /// build until it knows the block geometry it has not seen yet.
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            open: None,
            last_closed: None,
            tally: Tally::default(),
            completed: 0,
        }
    }

    /// Consume one validated record's body and say what it did.
    pub fn accept(&mut self, body: &[u8]) -> Actions {
        self.tally.records += 1;
        let mut actions = Actions::none();
        if body.starts_with(AUDIO_PREFIX) {
            self.consume_chunk(body, &mut actions);
        } else if body.starts_with(AUDEND_PREFIX) {
            self.consume_summary(body, &mut actions);
        } else {
            self.report(Action::Ignored, &mut actions);
        }
        actions
    }

    /// End the stream: report the block that was still open and hand over the counters.
    pub fn finish(self) -> Finish {
        let mut tally = self.tally;
        let abandoned = match self.open {
            Some(open) => {
                // Charged here rather than through `Tally::observe`, which takes a reference to an
                // event this assembler no longer needs to describe to anyone but the counters.
                tally.blocks_abandoned += 1;
                Some(Abandoned {
                    block: open.block,
                    missing: open.missing(),
                })
            }
            None => None,
        };
        Finish { abandoned, tally }
    }

    /// The most recently completed block's raw bytes: mono 16-bit capture, ready to interleave or
    /// hash unchanged.
    ///
    /// Meaningful between an [`Action::Complete`] and the moment another block starts, and empty
    /// otherwise. Handing these out through the assembler rather than on the action keeps the event
    /// `Copy` and avoids a second mutable view of the same buffer.
    pub fn pcm(&self) -> &[u8] {
        &self.buf[..self.completed]
    }

    /// Charge an event and record it for the caller, so no path can update one of the two views and
    /// forget the other.
    fn report(&mut self, action: Action, out: &mut Actions) {
        self.tally.observe(&action);
        out.push(action);
    }

    /// Begin a block, invalidating any bytes a previous completion left claimable.
    fn begin(&mut self, block: u32, chunks: u16) -> OpenBlock {
        self.completed = 0;
        OpenBlock::new(block, chunks)
    }

    /// Handle an `AUDIO` body: parse, resolve which block it belongs to, then store or refuse.
    fn consume_chunk(&mut self, body: &[u8], out: &mut Actions) {
        let chunk = match parse_chunk(body) {
            Ok(chunk) => chunk,
            Err(at) => {
                self.report(Action::Malformed { at }, out);
                return;
            }
        };

        // Grammar before geometry: `n = 0` has no chunk zero to receive, and an index at or past `n`
        // claims a place in a block that says it has none.
        if chunk.chunks == 0 {
            self.report(Action::Malformed { at: CHUNK_COUNT_AT }, out);
            return;
        }
        if chunk.index >= chunk.chunks {
            self.report(Action::Malformed { at: CHUNK_INDEX_AT }, out);
            return;
        }

        // Identity resolves before the payload is decoded, so a record belonging to a block that has
        // already closed is called late whether or not its contents would have parsed.
        let prior = self.open.take();
        let mut block = match prior {
            Some(open) if open.block == chunk.block => {
                if open.chunks != chunk.chunks {
                    let block = open.block;
                    self.last_closed = Some(block);
                    self.completed = 0;
                    let reason = Failure::ChunkCount {
                        expected: open.chunks,
                        found: chunk.chunks,
                    };
                    self.report(Action::Failed { block, reason }, out);
                    return;
                }
                open
            }
            Some(open) => {
                let missing = open.missing();
                self.report(
                    Action::Abandoned {
                        block: open.block,
                        missing,
                    },
                    out,
                );
                self.begin(chunk.block, chunk.chunks)
            }
            None => {
                if self.last_closed == Some(chunk.block) {
                    self.report(
                        Action::LateRecord {
                            block: chunk.block,
                            chunk: Some(chunk.index),
                        },
                        out,
                    );
                    return;
                }
                self.begin(chunk.block, chunk.chunks)
            }
        };

        // Strict decoding happens here rather than in the caller: a payload that is not canonical
        // base64 is corruption the frame checksum let through. Such a chunk is not kept and the
        // block is left exactly as it was, because the honest consequence of a chunk nobody can read
        // is that the index is missing — which fails the block at its summary with the missing list
        // naming it. Refusing the block here instead would invent a second way for one to die and
        // lose the diagnosis the missing list carries.
        let mut raw = [0u8; CHUNK_RAW];
        let written = match decode_payload(chunk.payload, &mut raw) {
            Ok(written) => written,
            Err(at) => {
                self.open = Some(block);
                self.report(Action::Malformed { at }, out);
                return;
            }
        };

        let action = place(&mut block, self.buf, chunk.index, &raw[..written]);
        // Only a refusal ends the block: storing, duplicating, and conflicting all leave it waiting
        // for the rest of its chunks.
        if keeps_open(&action) {
            self.open = Some(block);
        } else {
            self.last_closed = Some(block.block);
            self.completed = 0;
        }
        self.report(action, out);
    }

    /// Handle an `AUDEND` body: decide the block it names, or refuse a block no chunk ever reached.
    fn consume_summary(&mut self, body: &[u8], out: &mut Actions) {
        let summary = match parse_summary(body) {
            Ok(summary) => summary,
            Err(at) => {
                self.report(Action::Malformed { at }, out);
                return;
            }
        };
        if summary.chunks == 0 {
            self.report(
                Action::Malformed {
                    at: SUMMARY_COUNT_AT,
                },
                out,
            );
            return;
        }

        let open = match self.open.take() {
            Some(open) if open.block == summary.block => open,
            Some(open) => {
                let missing = open.missing();
                self.report(
                    Action::Abandoned {
                        block: open.block,
                        missing,
                    },
                    out,
                );
                self.refuse_without_chunks(&summary, out);
                return;
            }
            None => {
                if self.last_closed == Some(summary.block) {
                    self.report(
                        Action::LateRecord {
                            block: summary.block,
                            chunk: None,
                        },
                        out,
                    );
                    return;
                }
                self.refuse_without_chunks(&summary, out);
                return;
            }
        };

        let action = verdict(&open, &summary, self.buf);
        self.last_closed = Some(summary.block);
        self.completed = match action {
            Action::Complete { bytes, .. } => bytes,
            _ => 0,
        };
        self.report(action, out);
    }

    /// Refuse a block a summary describes but no chunk ever reached: `n` says how many were owed and
    /// none arrived, so the missing list is the whole block.
    fn refuse_without_chunks(&mut self, summary: &SummaryFields, out: &mut Actions) {
        let mut missing = MissingChunks::NONE;
        for index in 0..summary.chunks {
            missing.insert(index);
        }
        self.last_closed = Some(summary.block);
        self.completed = 0;
        self.report(
            Action::Failed {
                block: summary.block,
                reason: Failure::Missing(missing),
            },
            out,
        );
    }
}

/// Store a decoded chunk in its geometric slot, or explain why the block is over.
///
/// Deliberately free of `self`: the staging buffer and the block's own state get decided together
/// here, and the caller only has to put the block back if it survived.
fn place(block: &mut OpenBlock, buf: &mut [u8], index: u16, raw: &[u8]) -> Action {
    let start = index as usize * CHUNK_RAW;
    let end = start + raw.len();
    let id = block.block;
    if end > buf.len() {
        return Action::Failed {
            block: id,
            reason: Failure::Capacity { needed: end },
        };
    }

    // Duplicates are decided by content, not by having seen the index before: a retransmission of
    // identical bytes is the retry a lossy link expects, while the same index carrying different
    // bytes means two writers, or one writer and a corrupted copy, and no amount of preferring the
    // newer arrival makes either one trustworthy.
    if block.is_present(index) {
        return if buf[start..end] == *raw {
            Action::Duplicate {
                block: id,
                chunk: index,
            }
        } else {
            if block.conflict.is_none() {
                block.conflict = Some(index);
            }
            Action::Conflict {
                block: id,
                chunk: index,
            }
        };
    }

    buf[start..end].copy_from_slice(raw);
    block.mark_present(index);
    block.placed += raw.len();
    if index + 1 == block.chunks {
        block.final_len = raw.len();
    }
    Action::ChunkStored {
        block: id,
        chunk: index,
        bytes: raw.len(),
    }
}

/// Whether a chunk-path event leaves the block waiting for more chunks.
fn keeps_open(action: &Action) -> bool {
    matches!(
        action,
        Action::ChunkStored { .. } | Action::Duplicate { .. } | Action::Conflict { .. }
    )
}

/// Decide a block whose summary has arrived.
///
/// Ordered by how much each refusal explains: a conflict or a missing list says where to look, a
/// checksum mismatch only says the bytes are not the ones that were sent.
fn verdict(open: &OpenBlock, summary: &SummaryFields, pcm: &[u8]) -> Action {
    let id = open.block;
    if open.chunks != summary.chunks {
        return Action::Failed {
            block: id,
            reason: Failure::ChunkCount {
                expected: open.chunks,
                found: summary.chunks,
            },
        };
    }
    if let Some(index) = open.conflict {
        return Action::Failed {
            block: id,
            reason: Failure::Conflict { index },
        };
    }
    if !open.complete() {
        return Action::Failed {
            block: id,
            reason: Failure::Missing(open.missing()),
        };
    }
    let expected = (open.chunks as usize - 1) * CHUNK_RAW + open.final_len;
    if open.placed != expected || open.placed != summary.total_bytes as usize {
        return Action::Failed {
            block: id,
            reason: Failure::Length {
                declared: summary.total_bytes,
                placed: open.placed,
                expected,
            },
        };
    }
    let computed = crc16_ccitt(&pcm[..open.placed]);
    if computed != summary.crc {
        return Action::Failed {
            block: id,
            reason: Failure::Crc {
                shipped: summary.crc,
                computed,
            },
        };
    }
    Action::Complete {
        block: id,
        bytes: open.placed,
    }
}

/// Strictly decode a chunk payload into `raw`; on refusal, the body offset to blame.
///
/// Room is checked against the geometry before decoding, so a body longer than the grammar allows is
/// refused as malformed rather than as an undersized output buffer — the buffer is sized by
/// [`CHUNK_RAW`], which is the same fact stated twice.
fn decode_payload(payload: &[u8], raw: &mut [u8]) -> Result<usize, usize> {
    if max_raw_for(payload.len()) > CHUNK_RAW {
        return Err(CHUNK_PAYLOAD_AT);
    }
    match decode(payload, raw) {
        // A payload with no bytes carries no information and no writer emits one.
        Ok(0) | Err(DecodeError::OutputTooSmall { .. }) => Err(CHUNK_PAYLOAD_AT),
        Ok(written) => Ok(written),
        Err(error) => Err(payload_offset(error_offset(&error))),
    }
}

/// Body offset a [`DecodeError`] complains about, given where the payload starts in the body.
fn payload_offset(payload_at: usize) -> usize {
    CHUNK_PAYLOAD_AT + payload_at
}

/// The byte a [`DecodeError`] points at, for the variants that name one.
fn error_offset(error: &DecodeError) -> usize {
    match error {
        DecodeError::Char { at, .. }
        | DecodeError::Padding { at }
        | DecodeError::TrailingBits { at } => *at,
        // A string that is not a whole number of groups is wrong as a whole, and an output buffer
        // too small for a payload the grammar bounds cannot happen: name the payload's start either
        // way rather than invent an offset the error did not report.
        DecodeError::Length(_) | DecodeError::OutputTooSmall { .. } => 0,
    }
}
