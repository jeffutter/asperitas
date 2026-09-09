//! `dump_reassemble` — turn a captured console byte stream back into raw PCM, and refuse every
//! block whose completeness cannot be proved.
//!
//! Run it on a file, or pipe a capture through it:
//!
//! ```sh
//! cargo run -p asperitas-logging --example dump_reassemble -- capture.bin --out capture.pcm
//! cat capture.bin | cargo run -p asperitas-logging --example dump_reassemble --out live.pcm
//! cargo run -p asperitas-logging --example dump_reassemble -- --selftest
//! ```
//!
//! Bytes are read in 4 KiB pieces — larger than one frame on purpose, so record boundaries never
//! land on chunk boundaries by luck — and every byte is either assembled into a block or charged to
//! the summary. `--selftest` generates synthetic captures, including deliberately damaged ones, runs
//! them through this same decoder-and-assembler path, and asserts each expected outcome: CI executes
//! it so the refusal behaviour is checked end to end with no board attached.
//!
//! # How this differs from `console_decode`, and why
//!
//! [`console_decode`](../examples/console_decode.rs) prints validated records to stdout and always
//! exits 0 on a readable capture, because deciding whether a capture passed is a human call. This
//! tool cannot keep either convention, and the divergence is deliberate rather than an oversight for
//! someone to "fix":
//!
//! - **Nothing goes to stdout.** PCM is not text; a WAV-less byte soup on a terminal is useless and
//!   would corrupt whatever terminal it landed in. Raw samples go to `--out <file>`, and the manifest
//!   — what was assembled, what was refused, and the numbers behind both — goes to stderr with the
//!   `dump_reassemble: ` prefix, so it interleaves safely with any redirection.
//! - **The exit code is a verdict.** `0` means every block this capture claimed, it proved. `1`
//!   means some block was incomplete, conflicted, short, or checksum-wrong, or a frame failed its own
//!   CRC. `2` means the input could not be read at all, which is not a clean capture — it is an
//!   unexamined one. A rig that gates on this tool needs a code that means *no*, so reporting-only
//!   semantics would make every scripted measurement vacuously green.
//!
//! What is copied unchanged from `console_decode`: the chunked read/push/drain handshake, tolerance
//! of a closed output file, and the accounting law — every byte offered is either inside a validated
//! record, charged to `discarded_bytes`, or still buffered. That law is printed rather than merely
//! obeyed, because the counters mean nothing unless the bytes they describe are all present somewhere
//! in the report.

use std::env;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::exit;

use asperitas_logging::dump::{
    self, Abandoned, Action, BlockAssembler, Failure, Finish, Tally, CHUNK_RAW,
    FULL_AUDIO_FRAME_LEN, MAX_CHUNKS_PER_BLOCK,
};
use asperitas_logging::frame::{encode, Decoder, Stats, MAX_BODY, MAX_FRAME};
use asperitas_logging::Level;

/// Bytes requested per read. Larger than [`MAX_FRAME`] on purpose: a reader that happens to align
/// with record boundaries proves nothing about the decoder's boundaries.
const CHUNK_BYTES: usize = 4096;

/// Staging room for one maximum-size block: every chunk the `n` field can name, at full width.
///
/// Sized from the grammar rather than guessed, so widening the chunk geometry widens the buffer with
/// it and a capture can never be refused for `Capacity` because this tool guessed small.
const STAGING_BYTES: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

fn main() {
    let options = match Options::parse(env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("dump_reassemble: {message}");
            exit(2);
        }
    };

    if options.selftest {
        exit(selftest::run());
    }

    let mut input = match options.open_input() {
        Ok(input) => input,
        Err(message) => {
            eprintln!("dump_reassemble: {message}");
            exit(2);
        }
    };

    // PCM goes to a file or nowhere; stdout stays empty by design (see the module documentation).
    let mut pcm_file: Option<BufWriter<File>> = options.out_path.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|e| fail_with(2, &format!("cannot write {path}: {e}"))),
        )
    });

    let mut sink =
        WriteOrDiscard::to_file(pcm_file.as_mut().map(|writer| writer as &mut dyn Write));
    let summary = consume(input.as_mut(), &mut sink);
    sink.finish();

    report(&summary, options.out_path.as_deref());

    // Unreadable input outranks anything the capture itself did: a half-read capture proves nothing,
    // and exit 2 says "look again" rather than "it failed". Otherwise any block that could not prove
    // itself, or arithmetic that does not balance, is a verdict of 1.
    if summary.unread {
        exit(2);
    }
    if summary.verdict_is_dirty() || !summary.law_holds() {
        exit(1);
    }
    exit(0);
}

/// Where the bytes come from and where the samples go.
struct Options {
    input_path: Option<String>,
    out_path: Option<String>,
    selftest: bool,
}

impl Options {
    /// Parse `[--out <file>] [<capture-file>] [--selftest]`, positionally for the input and by flag
    /// for the output so a capture piped on stdin needs no argument at all.
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Options {
            input_path: None,
            out_path: None,
            selftest: false,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => {
                    let path = args
                        .next()
                        .ok_or_else(|| "--out needs a file name".to_string())?;
                    options.out_path = Some(path);
                }
                "--selftest" => options.selftest = true,
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other}"));
                }
                other => {
                    if options.input_path.is_some() {
                        return Err(format!("second input file given: {other}"));
                    }
                    options.input_path = Some(other.to_string());
                }
            }
        }
        Ok(options)
    }

    fn open_input(&self) -> io::Result<Box<dyn Read>> {
        match &self.input_path {
            None => Ok(Box::new(io::stdin())),
            Some(path) => match File::open(path) {
                Ok(file) => Ok(Box::new(BufReader::new(file))),
                Err(e) => Err(io::Error::new(e.kind(), format!("cannot read {path}: {e}"))),
            },
        }
    }
}

/// One completed block, as the manifest reports it.
#[derive(Debug)]
struct BlockRow {
    block: u32,
    bytes: usize,
    /// Offset of this block's first sample byte within the PCM output.
    offset: u64,
}

/// Everything one stream produced, in the terms the manifest and the exit code need.
#[derive(Debug)]
struct Summary {
    pushed: u64,
    framed_bytes: u64,
    stats: Stats,
    buffered: usize,
    tally: Tally,
    blocks: Vec<BlockRow>,
    refused: Vec<(u32, Failure)>,
    abandoned: Vec<Abandoned>,
    /// Set when the input itself could not be read to the end: this describes the tool's failure to
    /// look at all the capture, kept apart from every counter that describes the capture.
    unread: bool,
}

impl Summary {
    /// Every byte offered is inside a validated record, charged to `discarded_bytes`, or buffered.
    fn law_holds(&self) -> bool {
        self.pushed == self.framed_bytes + self.stats.discarded_bytes + self.buffered as u64
    }

    /// Whether anything in this capture failed to prove itself.
    ///
    /// Duplicates are absent on purpose: a byte-identical re-send is the retry a lossy link expects,
    /// not evidence of damage.
    fn verdict_is_dirty(&self) -> bool {
        self.tally.blocks_failed > 0
            || self.tally.blocks_abandoned > 0
            || self.tally.conflicts > 0
            || self.tally.malformed_records > 0
            || self.tally.late_records > 0
            || self.stats.bad_frames > 0
    }
}

/// PCM destination that tolerates being absent, and a closed file.
///
/// A failed write stops the samples but not the counting: the run decodes to the end so the summary
/// describes the whole capture rather than wherever the disk gave up. Halting both together would
/// report the unread remainder as loss, which is a number this program exists to get right.
struct WriteOrDiscard<'a> {
    file: Option<&'a mut dyn Write>,
    /// Kept apart from `file` instead of being its fallback: a capture with no `--out` is asking what
    /// arrived, not asking to hold hundreds of megabytes of samples until the machine swaps.
    memory: Option<&'a mut Vec<u8>>,
    broken: bool,
}

impl<'a> WriteOrDiscard<'a> {
    /// Send samples to `file`, or drop them when there is none.
    fn to_file(file: Option<&'a mut dyn Write>) -> Self {
        Self {
            file,
            memory: None,
            broken: false,
        }
    }

    /// Catch samples in memory, for `--selftest` comparing one stream's output against another's.
    fn to_memory(memory: &'a mut Vec<u8>) -> Self {
        Self {
            file: None,
            memory: Some(memory),
            broken: false,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        match (self.file.as_mut(), self.memory.as_mut()) {
            (Some(file), _) => {
                if self.broken {
                    return;
                }
                if let Err(e) = file.write_all(bytes) {
                    if e.kind() != io::ErrorKind::BrokenPipe {
                        eprintln!("dump_reassemble: PCM write failed: {e}");
                    }
                    self.broken = true;
                }
            }
            (None, Some(memory)) => memory.extend_from_slice(bytes),
            (None, None) => {}
        }
    }

    fn finish(&mut self) {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
    }
}

/// Decode a whole stream and assemble its blocks.
///
/// One implementation for both the real capture path and `--selftest`, so the synthetic streams test
/// the same push/drain handshake the tool uses in anger rather than a model of it.
fn consume(input: &mut dyn Read, sink: &mut WriteOrDiscard<'_>) -> Summary {
    let mut decoder = Decoder::new();
    let mut staging = vec![0u8; STAGING_BYTES];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut chunk = [0u8; CHUNK_BYTES];
    let mut frame_buf = [0u8; MAX_FRAME];

    let mut got = Assembled::default();
    let mut pushed: u64 = 0;
    let mut unread = false;

    loop {
        let filled = match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("dump_reassemble: read failed: {e}");
                unread = true;
                break;
            }
        };

        // `push` takes only what the decoder can currently hold, so the rest of the chunk waits here
        // until the records inside it have been consumed. That handshake is what makes this loop
        // lossless however long the read.
        let mut offset = 0usize;
        while offset < filled {
            let before = offset;
            offset += decoder.push(&chunk[offset..filled]);
            drain(&mut decoder, &mut assembler, sink, &mut frame_buf, &mut got);
            if offset == before {
                // Unreachable while `next_record` is being drained: freeing a queue slot always lets
                // `push` take more input. Reported, not assumed.
                eprintln!("dump_reassemble: decoder took no further bytes; stopping early");
                unread = true;
                break;
            }
        }
        // Count what the decoder actually took. Bytes still waiting in this chunk when we stopped
        // early were never offered, so they belong to no counter.
        pushed += offset as u64;
    }

    // Required before reading the counters: whatever is still undecided has nowhere to come from, so
    // it becomes discarded bytes rather than an invisible remainder. The assembler's matching call
    // reports the block whose tail the stream never delivered.
    decoder.finish();
    drain(&mut decoder, &mut assembler, sink, &mut frame_buf, &mut got);
    let Finish { tally, abandoned } = assembler.finish();
    if let Some(abandoned) = abandoned {
        got.abandoned.push(abandoned);
    }

    Summary {
        pushed,
        framed_bytes: got.framed_bytes,
        stats: decoder.stats(),
        buffered: decoder.buffered(),
        tally,
        blocks: got.blocks,
        refused: got.refused,
        abandoned: got.abandoned,
        unread,
    }
}

/// What the assembler has produced so far, as the read loop accumulates it.
#[derive(Default)]
struct Assembled {
    /// Wire bytes of every record handed to the assembler, for the accounting law.
    framed_bytes: u64,
    blocks: Vec<BlockRow>,
    refused: Vec<(u32, Failure)>,
    abandoned: Vec<Abandoned>,
    /// Where the next completed block's samples begin in the output.
    pcm_offset: u64,
}

/// Hand every queued record to the assembler, charging each one's wire cost and writing out any block
/// it just finished.
///
/// One function rather than a copy at each call site: the read loop and the drain after
/// [`Decoder::finish`] must agree about what a completion means, or a record still queued when a read
/// gave up would lose its samples without saying so.
fn drain(
    decoder: &mut Decoder,
    assembler: &mut BlockAssembler<'_>,
    sink: &mut WriteOrDiscard<'_>,
    frame_buf: &mut [u8; MAX_FRAME],
    got: &mut Assembled,
) {
    while let Some(record) = decoder.next_record() {
        got.framed_bytes += reencoded_frame(&record, frame_buf).len() as u64;
        let actions = assembler.accept(record.body);
        // The body borrow ends with the record, so the assembler's PCM slice and the next record never
        // coexist: copy out whatever a completion promises before asking for anything else.
        for action in actions.as_slice() {
            match *action {
                Action::Complete { block, bytes } => {
                    let pcm = assembler.pcm();
                    debug_assert_eq!(pcm.len(), bytes);
                    sink.write(pcm);
                    got.blocks.push(BlockRow {
                        block,
                        bytes,
                        offset: got.pcm_offset,
                    });
                    got.pcm_offset += bytes as u64;
                }
                Action::Failed { block, reason } => got.refused.push((block, reason)),
                Action::Abandoned { block, missing } => {
                    got.abandoned.push(Abandoned { block, missing })
                }
                // Non-dump records still occupy the console stream; echo the few that matter to a human
                // scanning a live capture rather than the megabytes of audio.
                Action::Ignored => note(record.body),
                _ => {}
            }
        }
    }
}

/// Echo a non-dump record's body when it carries a fact a rig watches: `STATUS` is the counter
/// stream, everything else is ordinary logging noise that would bury the manifest.
fn note(body: &[u8]) {
    if body.starts_with(b"STATUS") {
        if let Ok(text) = core::str::from_utf8(body) {
            eprintln!("dump_reassemble: {text}");
        }
    }
}

/// Reassemble the wire bytes a validated record arrived as, to measure what it cost on the wire.
///
/// The record already passed CRC, so re-encoding its fields is not re-checking anything: the prefix
/// is fixed-width, the body arrives pre-sanitised, and the checksum is a function of the two, so this
/// reproduces the frame byte for byte.
fn reencoded_frame<'a>(
    record: &asperitas_logging::frame::Record<'_>,
    buf: &'a mut [u8; MAX_FRAME],
) -> &'a [u8] {
    let encoded = encode(Level::Info, record.seq, record.t_ms, record.body, buf);
    &buf[..encoded.len]
}

/// Print the manifest: what was assembled, what was refused, and the arithmetic behind both.
fn report(summary: &Summary, out_path: Option<&str>) {
    let destination = out_path.unwrap_or("nowhere, because no --out was given");
    eprintln!(
        "dump_reassemble: blocks complete={} failed={} abandoned={}",
        summary.tally.blocks_completed, summary.tally.blocks_failed, summary.tally.blocks_abandoned
    );
    eprintln!(
        "dump_reassemble: chunks stored={} duplicate={} conflict={} late={}",
        summary.tally.chunks_stored,
        summary.tally.duplicate_chunks,
        summary.tally.conflicts,
        summary.tally.late_records
    );
    eprintln!(
        "dump_reassemble: records decoded={} bad_frames={} resyncs={} discarded_bytes={} non_dump={} malformed={}",
        summary.stats.records,
        summary.stats.bad_frames,
        summary.stats.resyncs,
        summary.stats.discarded_bytes,
        summary.tally.non_dump_records,
        summary.tally.malformed_records
    );

    for row in &summary.blocks {
        eprintln!(
            "dump_reassemble: block {:04x}: {} bytes at pcm offset {}",
            row.block, row.bytes, row.offset
        );
    }
    for (block, reason) in &summary.refused {
        eprintln!("dump_reassemble: block {block:04x} refused: {reason:?}");
    }
    for abandoned in &summary.abandoned {
        eprintln!(
            "dump_reassemble: block {:04x} abandoned: {:?}",
            abandoned.block, abandoned.missing
        );
    }

    let accounted = summary.framed_bytes + summary.stats.discarded_bytes + summary.buffered as u64;
    eprintln!(
        "dump_reassemble: bytes pushed={} accounted_for={} (framed_records={} discarded={} buffered={})",
        summary.pushed, accounted, summary.framed_bytes, summary.stats.discarded_bytes, summary.buffered
    );
    if !summary.law_holds() {
        eprintln!(
            "dump_reassemble: ACCOUNTING FAILURE: {} bytes are in neither category",
            summary.pushed.saturating_sub(accounted)
        );
    }

    // Useful bytes per wire byte, measured against the ceiling the grammar allows. A capture that
    // lands far below the theoretical figure lost payload to damage or to padding, and the gap is
    // worth seeing without opening a spreadsheet.
    let measured = if summary.pushed == 0 {
        0.0
    } else {
        summary.tally.pcm_bytes as f64 / summary.pushed as f64
    };
    let theoretical = CHUNK_RAW as f64 / FULL_AUDIO_FRAME_LEN as f64;
    eprintln!(
        "dump_reassemble: useful bytes per wire byte: measured {measured:.4}, at most {theoretical:.4} \
         ({CHUNK_RAW} raw in a {FULL_AUDIO_FRAME_LEN}-byte frame; padding and shorter final chunks lower it)"
    );
    eprintln!(
        "dump_reassemble: pcm bytes written to {destination}: {}",
        summary.tally.pcm_bytes
    );
    eprintln!(
        "dump_reassemble: note: a block is complete only when every chunk it promised arrived and the \
         block checksum matches the assembled bytes. A CRC verifies integrity, never absence."
    );
}

/// Report a fatal problem with this program's own operation and stop.
fn fail_with(code: i32, message: &str) -> ! {
    eprintln!("dump_reassemble: {message}");
    exit(code);
}

// ---------------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------------

/// Synthetic captures and the verdicts each one must produce.
///
/// Every case runs through [`consume`], so what is under test is the shipped path: the read loop, the
/// decoder, the assembler, and the exit-code rule beneath `main`. No board, no files, no tempdirs.
///
/// Damage is applied to raw samples *before* framing, never to bytes already on the wire. A bit flip
/// landing on a shipped record almost always trips that record's own CRC, so the frame is dropped and
/// the assembler never sees the damage — right behaviour, wrong layer being tested. Corrupting before
/// encoding reproduces what a device fault actually looks like: a frame that validates perfectly while
/// carrying wrong samples, catchable only by the block checksum and the missing list.
mod selftest {
    use super::*;
    use asperitas_logging::frame::crc16_ccitt;

    /// Chunks per generated block: two at full width plus a short tail, which exercises the
    /// padded-final-chunk path and the length arithmetic in one shape.
    const BLOCK_CHUNKS: u16 = 3;

    /// Raw bytes in a generated block's final chunk: deliberately not [`CHUNK_RAW`], because the
    /// grammar says the last chunk is the short one.
    const TAIL_BYTES: usize = 40;

    /// Blocks per generated stream. Three is the smallest number where one block can die while the
    /// blocks either side of it survive, which is the shape most expectations below assert.
    const BLOCKS: u64 = 3;

    /// Raw bytes one generated block carries.
    const BLOCK_BYTES: usize = (BLOCK_CHUNKS as usize - 1) * CHUNK_RAW + TAIL_BYTES;

    /// Block that loses a whole record, and block whose samples get corrupted: both sit inside the
    /// stream, so neighbours still complete and a refusal cannot hide behind a total collapse.
    const MIDDLE_BLOCK: u32 = 1;
    const FIRST_BLOCK: u32 = 0;

    /// Block ids used only by the single-block cases, kept clear of `0..BLOCKS`.
    const AUDEND_ONLY_BLOCK: u32 = 7;
    const LATE_BLOCK: u32 = 3;

    /// What a case asserts: its summary, the samples that case assembled, and the samples the clean
    /// stream assembled. Cases that care about sample bytes compare the two; the rest ignore both.
    type Check = fn(summary: &Summary, produced: &[u8], expected: &[u8]) -> Result<(), String>;

    pub(super) fn run() -> i32 {
        let cases: [(&str, Vec<u8>, Check); 9] = [
            ("clean", clean_stream(), expect_clean),
            (
                "record-deleted",
                deleted_record_stream(),
                expect_one_block_missing,
            ),
            (
                "samples-corrupted",
                corrupted_stream(),
                expect_corruption_refused,
            ),
            ("chunks-reordered", reordered_stream(), expect_reordered),
            (
                "start-markers-stripped",
                stripped_stream(),
                expect_nothing_completes,
            ),
            ("audend-alone", audend_only_stream(), expect_audend_alone),
            (
                "chunk-duplicated",
                duplicated_stream(),
                expect_duplicate_idempotent,
            ),
            (
                "chunk-duplicated-flipped",
                conflict_stream(),
                expect_conflict_refused,
            ),
            (
                "late-chunk-after-close",
                late_chunk_stream(),
                expect_late_reported,
            ),
        ];
        let expected = clean_pcm();

        let mut failures = 0usize;
        for (name, stream, check) in &cases {
            let mut input: &[u8] = stream;
            let mut produced = Vec::new();
            let summary = {
                let mut sink = WriteOrDiscard::to_memory(&mut produced);
                consume(&mut input, &mut sink)
            };
            match check(&summary, &produced, &expected) {
                Ok(()) => eprintln!("dump_reassemble: selftest {name} ok"),
                Err(reason) => {
                    eprintln!("dump_reassemble: selftest {name} FAILED: {reason}");
                    failures += 1;
                }
            }
        }

        if failures == 0 {
            eprintln!("dump_reassemble: selftest passed ({} cases)", cases.len());
            0
        } else {
            eprintln!(
                "dump_reassemble: selftest FAILED ({failures} of {} cases)",
                cases.len()
            );
            1
        }
    }

    // ── Stream construction ───────────────────────────────────────────

    /// One block's records, kept apart so a case can drop, repeat, reverse, or re-send one piece
    /// without disturbing any other block.
    struct Block {
        /// Block id as it appears on the wire.
        id: u32,
        /// Chunk records in send order.
        chunks: Vec<Vec<u8>>,
        /// The summary closing this block.
        summary: Vec<u8>,
        /// Raw samples this block's summary checksums.
        raw: Vec<u8>,
    }

    /// Raw bytes chunk `index` carries: full width except the final chunk.
    fn chunk_len(index: u16) -> usize {
        if index + 1 == BLOCK_CHUNKS {
            TAIL_BYTES
        } else {
            CHUNK_RAW
        }
    }

    /// Raw samples for chunk `index` of `block`: deterministic, different for every block and index,
    /// and spanning the whole byte range across a block so mixing two payloads could not slip by.
    fn payload(block: u32, index: u16) -> Vec<u8> {
        (0..chunk_len(index))
            .map(|byte| (((index as usize * 7 + byte) % 256) as u8) ^ block as u8)
            .collect()
    }

    /// Build one block: its chunks, then the summary checksumming the samples as they should be.
    ///
    /// `corrupt` names a byte of the block's samples to flip **before encoding**, so every frame stays
    /// valid while the assembled block stops matching its own checksum. Records are numbered from
    /// `seq`, which advances as the device's would.
    fn build_block(seq: &mut u32, block: u32, corrupt: Option<usize>) -> Block {
        let mut honest = Vec::with_capacity(BLOCK_BYTES);
        for index in 0..BLOCK_CHUNKS {
            honest.extend_from_slice(&payload(block, index));
        }

        let mut sent = honest.clone();
        if let Some(at) = corrupt {
            sent[at] ^= 0xff;
        }

        let mut chunks = Vec::with_capacity(BLOCK_CHUNKS as usize);
        for index in 0..BLOCK_CHUNKS {
            let start = index as usize * CHUNK_RAW;
            chunks.push(chunk_record(
                seq,
                block,
                index,
                &sent[start..start + chunk_len(index)],
            ));
        }
        let summary = summary_record(seq, block, &honest);
        Block {
            id: block,
            chunks,
            summary,
            raw: honest,
        }
    }

    /// Wire bytes of one `AUDIO` record.
    fn chunk_record(seq: &mut u32, block: u32, index: u16, raw: &[u8]) -> Vec<u8> {
        let mut body = [0u8; MAX_BODY];
        let mut frame_buf = [0u8; MAX_FRAME];
        let encoded = dump::audio_record(
            Level::Info,
            *seq,
            *seq * 4,
            block,
            BLOCK_CHUNKS,
            index,
            raw,
            &mut body,
            &mut frame_buf,
        )
        .expect("generated chunk fields are inside the grammar");
        *seq += 1;
        frame_buf[..encoded.len].to_vec()
    }

    /// Wire bytes of the `AUDEND` closing `block`, checksumming `raw`.
    fn summary_record(seq: &mut u32, block: u32, raw: &[u8]) -> Vec<u8> {
        let mut body = [0u8; MAX_BODY];
        let mut frame_buf = [0u8; MAX_FRAME];
        let encoded = dump::audend_record(
            Level::Info,
            *seq,
            *seq * 4,
            block,
            BLOCK_CHUNKS,
            raw.len() as u32,
            crc16_ccitt(raw),
            &mut body,
            &mut frame_buf,
        )
        .expect("generated summary fields are inside the grammar");
        *seq += 1;
        frame_buf[..encoded.len].to_vec()
    }

    /// The blocks of a healthy capture, optionally with one block's samples corrupted.
    fn blocks(corrupt: Option<u32>) -> Vec<Block> {
        let mut seq = 1u32;
        (0..BLOCKS as u32)
            .map(|block| {
                let damaged = if Some(block) == corrupt {
                    Some(0)
                } else {
                    None
                };
                build_block(&mut seq, block, damaged)
            })
            .collect()
    }

    /// Concatenate blocks in send order.
    fn flatten(blocks: Vec<Block>) -> Vec<u8> {
        let mut stream = Vec::new();
        for block in blocks {
            for record in &block.chunks {
                stream.extend_from_slice(record);
            }
            stream.extend_from_slice(&block.summary);
        }
        stream
    }

    /// The reference stream: every block sent as a healthy device sends it.
    fn clean_stream() -> Vec<u8> {
        flatten(blocks(None))
    }

    /// The samples that stream must assemble.
    fn clean_pcm() -> Vec<u8> {
        let mut pcm = Vec::new();
        for block in blocks(None) {
            pcm.extend_from_slice(&block.raw);
        }
        pcm
    }

    /// The clean stream's samples with block `gone` left out: the exact output a refusal must
    /// produce, which is how the cases below prove a refused block contributes none of its chunks
    /// rather than contributing the ones that happened to arrive.
    fn expected_without(gone: usize) -> Vec<u8> {
        let all = clean_pcm();
        let mut out = Vec::with_capacity(all.len() - BLOCK_BYTES);
        for block in 0..BLOCKS as usize {
            if block == gone {
                continue;
            }
            let start = block * BLOCK_BYTES;
            out.extend_from_slice(&all[start..start + BLOCK_BYTES]);
        }
        out
    }

    /// One whole record deleted: the middle chunk of the middle block. Exactly the loss a ring-buffer
    /// wrap produces, and exactly the loss a checksum over the bytes that did arrive cannot see.
    fn deleted_record_stream() -> Vec<u8> {
        let mut stream = Vec::new();
        for block in blocks(None) {
            for (index, record) in block.chunks.iter().enumerate() {
                if block.id == MIDDLE_BLOCK && index == 1 {
                    continue;
                }
                stream.extend_from_slice(record);
            }
            stream.extend_from_slice(&block.summary);
        }
        stream
    }

    /// One sample byte corrupted inside the first block. Every frame still validates, so only the
    /// block checksum stands between this capture and a false "complete".
    fn corrupted_stream() -> Vec<u8> {
        flatten(blocks(Some(FIRST_BLOCK)))
    }

    /// Every block's chunks delivered back-to-front, with each summary still closing its own block:
    /// arrival order differs from placement order for all chunks at once. Blocks stay sequential
    /// because one producer writing two of them at once is loss rather than concurrency (see
    /// [`BlockAssembler`]).
    fn reordered_stream() -> Vec<u8> {
        let mut stream = Vec::new();
        for block in blocks(None) {
            for record in block.chunks.iter().rev() {
                stream.extend_from_slice(record);
            }
            stream.extend_from_slice(&block.summary);
        }
        stream
    }

    /// Remove every `~`: the blind spot `frame.rs` documents, where a record that lost its start
    /// marker raises no integrity failure whatsoever. Bodies are sanitised on the way out, so nothing
    /// else in the stream can carry that byte.
    fn stripped_stream() -> Vec<u8> {
        clean_stream()
            .into_iter()
            .filter(|byte| *byte != b'~')
            .collect()
    }

    /// A summary whose block never had a single chunk sent. Its id is nobody else's, so no leftover
    /// state can be mistaken for its chunks.
    fn audend_only_stream() -> Vec<u8> {
        let mut seq = 1u32;
        build_block(&mut seq, AUDEND_ONLY_BLOCK, None).summary
    }

    /// A clean stream with one chunk per block repeated verbatim immediately after itself: the retry
    /// a lossy link expects.
    fn duplicated_stream() -> Vec<u8> {
        let mut stream = Vec::new();
        for block in blocks(None) {
            for (index, record) in block.chunks.iter().enumerate() {
                stream.extend_from_slice(record);
                if index == 0 {
                    stream.extend_from_slice(record);
                }
            }
            stream.extend_from_slice(&block.summary);
        }
        stream
    }

    /// A clean stream with one chunk per block re-sent carrying *different* samples under a frame that
    /// validates. Two candidate shapes for one index, which no preference for the newer arrival makes
    /// trustworthy.
    fn conflict_stream() -> Vec<u8> {
        let mut seq = 1u32;
        let mut stream = Vec::new();
        for block in 0..BLOCKS as u32 {
            let built = build_block(&mut seq, block, None);
            for (index, record) in built.chunks.iter().enumerate() {
                stream.extend_from_slice(record);
                if index == 0 {
                    let mut flipped = payload(block, 0);
                    flipped[0] ^= 0x5a;
                    stream.extend_from_slice(&chunk_record(&mut seq, block, 0, &flipped));
                }
            }
            stream.extend_from_slice(&built.summary);
        }
        stream
    }

    /// A block that completed, followed by a re-send of one of its chunks after its summary.
    fn late_chunk_stream() -> Vec<u8> {
        let mut seq = 1u32;
        let block = build_block(&mut seq, LATE_BLOCK, None);
        let mut stream = Vec::new();
        for record in &block.chunks {
            stream.extend_from_slice(record);
        }
        stream.extend_from_slice(&block.summary);
        stream.extend_from_slice(&block.chunks[1]);
        stream
    }

    // ── Expectations ──────────────────────────────────────────────────

    /// Nothing damaged: every block proved itself and the samples are the ones that were sent.
    fn expect_clean(summary: &Summary, produced: &[u8], expected: &[u8]) -> Result<(), String> {
        require(
            summary.tally.blocks_completed == BLOCKS,
            "all blocks complete",
        )?;
        require(summary.refused.is_empty(), "nothing refused")?;
        require(summary.abandoned.is_empty(), "nothing abandoned")?;
        require(!summary.verdict_is_dirty(), "a clean capture is clean")?;
        require(summary.law_holds(), "the accounting law balances")?;
        require(
            summary.blocks.len() == BLOCKS as usize,
            "one manifest row per block",
        )?;
        require(produced == expected, "assembled samples equal the source")
    }

    /// A deleted record is named by index, and the block around the hole yields no samples at all.
    fn expect_one_block_missing(
        summary: &Summary,
        produced: &[u8],
        _all: &[u8],
    ) -> Result<(), String> {
        require(
            summary.tally.blocks_completed == BLOCKS - 1,
            "two blocks complete",
        )?;
        require(summary.refused.len() == 1, "one block refused")?;
        let (block, reason) = summary.refused[0];
        require(
            block == MIDDLE_BLOCK,
            "the refused block is the one with a hole",
        )?;
        match reason {
            Failure::Missing(missing) => {
                require(missing.count() == 1, "exactly one chunk missing")?;
                require(
                    missing.contains(1),
                    "the missing list names the deleted chunk",
                )?;
            }
            other => return Err(format!("expected a missing-chunk refusal, got {other:?}")),
        }
        require(
            produced == expected_without(MIDDLE_BLOCK as usize),
            "a refused block hands out none of its bytes",
        )
    }

    /// Wrong samples inside valid frames reach the block checksum, which is the only thing left
    /// standing between them and a completed block.
    fn expect_corruption_refused(
        summary: &Summary,
        produced: &[u8],
        _all: &[u8],
    ) -> Result<(), String> {
        require(
            summary.stats.bad_frames == 0,
            "no frame complains: the damage rode in inside valid records",
        )?;
        require(
            summary.tally.blocks_completed == BLOCKS - 1,
            "two blocks survive",
        )?;
        require(summary.refused.len() == 1, "one block refused")?;
        let (block, reason) = summary.refused[0];
        require(
            block == FIRST_BLOCK,
            "the refused block is the corrupted one",
        )?;
        match reason {
            Failure::Crc { .. } => {}
            other => return Err(format!("expected a checksum refusal, got {other:?}")),
        }
        require(
            produced == expected_without(FIRST_BLOCK as usize),
            "the corrupted block hands out no samples",
        )
    }

    /// Placement at `chunk_index · CHUNK_RAW` means permutation is structurally impossible rather
    /// than merely unlikely: the same chunks in any order yield the same bytes.
    fn expect_reordered(summary: &Summary, produced: &[u8], expected: &[u8]) -> Result<(), String> {
        require(
            summary.tally.blocks_completed == BLOCKS,
            "all blocks complete",
        )?;
        require(summary.refused.is_empty(), "nothing refused")?;
        require(!summary.verdict_is_dirty(), "arrival order is not damage")?;
        require(
            produced == expected,
            "reversed arrival yields byte-identical samples",
        )
    }

    /// The documented blind spot made concrete: without start markers there is nothing to decode, and
    /// the only honest report is that every byte went away.
    fn expect_nothing_completes(
        summary: &Summary,
        produced: &[u8],
        _expected: &[u8],
    ) -> Result<(), String> {
        require(
            summary.tally.blocks_completed == 0,
            "stripping start markers completes nothing",
        )?;
        require(
            summary.tally.chunks_stored == 0,
            "no chunk reaches the assembler",
        )?;
        require(produced.is_empty(), "no samples handed out")?;
        require(summary.law_holds(), "the accounting law still balances")?;
        require(
            summary.pushed > 0 && summary.stats.discarded_bytes == summary.pushed,
            "every byte is charged to discarded_bytes",
        )
    }

    /// A summary alone proves nothing except what was owed.
    fn expect_audend_alone(
        summary: &Summary,
        produced: &[u8],
        _expected: &[u8],
    ) -> Result<(), String> {
        require(summary.tally.blocks_completed == 0, "nothing completes")?;
        require(summary.refused.len() == 1, "the lone summary is refused")?;
        match summary.refused[0].1 {
            Failure::Missing(missing) if missing.count() == BLOCK_CHUNKS as usize => require(
                missing.contains(0) && missing.contains(BLOCK_CHUNKS - 1),
                "the missing list spans the whole block",
            ),
            other => Err(format!("expected a full missing list, got {other:?}")),
        }?;
        require(produced.is_empty(), "no samples handed out")
    }

    /// A byte-identical retry changes nothing — not the counters that gate CI, not the samples.
    fn expect_duplicate_idempotent(
        summary: &Summary,
        produced: &[u8],
        expected: &[u8],
    ) -> Result<(), String> {
        require(
            summary.tally.duplicate_chunks == BLOCKS,
            "one retry recognised per block",
        )?;
        require(summary.tally.conflicts == 0, "a retry is not a conflict")?;
        require(
            summary.tally.blocks_completed == BLOCKS,
            "all blocks complete",
        )?;
        require(!summary.verdict_is_dirty(), "a retry is not damage")?;
        require(produced == expected, "identical samples")
    }

    /// The same index twice with different bytes leaves the block with two candidate shapes, so it
    /// gets no verdict but a refusal naming the index.
    fn expect_conflict_refused(
        summary: &Summary,
        produced: &[u8],
        _expected: &[u8],
    ) -> Result<(), String> {
        require(
            summary.tally.conflicts == BLOCKS,
            "one conflicting re-send per block",
        )?;
        require(
            summary.tally.blocks_completed == 0,
            "every block is refused",
        )?;
        require(
            summary.refused.len() == BLOCKS as usize,
            "one refusal per block",
        )?;
        for (block, reason) in &summary.refused {
            match reason {
                Failure::Conflict { index: 0 } => {}
                other => {
                    return Err(format!(
                        "block {block:04x}: expected a conflict, got {other:?}"
                    ))
                }
            }
        }
        require(
            produced.is_empty(),
            "a conflicted block hands out no samples",
        )
    }

    /// Completion is final: a chunk arriving afterwards is reported, and does not reopen a verdict
    /// already published.
    fn expect_late_reported(
        summary: &Summary,
        produced: &[u8],
        _expected: &[u8],
    ) -> Result<(), String> {
        require(
            summary.tally.blocks_completed == 1,
            "the block completed first",
        )?;
        require(
            summary.tally.late_records == 1,
            "the re-send is reported late",
        )?;
        require(
            summary.refused.is_empty(),
            "a late chunk does not undo completion",
        )?;
        require(
            produced.len() == BLOCK_BYTES,
            "the completed block's samples stand",
        )
    }

    fn require(condition: bool, what: &str) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(format!("expected: {what}"))
        }
    }
}
