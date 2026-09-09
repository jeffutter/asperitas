//! `console_decode` — turn a raw USB console capture into validated records plus a
//! capture summary whose numbers add up.
//!
//! Run it on a file, or pipe a capture through it:
//!
//! ```sh
//! cargo run -p asperitas-logging --example console_decode -- capture.bin > clean.txt
//! cat capture.bin | cargo run -p asperitas-logging --example console_decode
//! ```
//!
//! Bytes are read in 4 KiB pieces — deliberately larger than one frame, so record
//! boundaries never land on chunk boundaries by luck — and every byte is either printed
//! as a validated record or charged to the summary. Nothing in between.
//!
//! stdout carries the records themselves, one per line, byte-identical to what the
//! device sent, so the output greps like the capture and feeds back into this same
//! program. stderr carries the four integrity counters and a sequence-continuity
//! summary. They answer different questions and are kept apart for that reason: the
//! counters say what the wire did to the bytes, the sequence summary says whether any
//! bytes are missing, and **a CRC cannot answer the second question** — a record that
//! never arrived leaves no trace in `bad_frames`. Only `seq` reveals it, and only a
//! person decides whether that gap was a stall, a dropped ring buffer, or the boot that
//! restarted the numbering.
//!
//! Exits 0 whatever the capture contains. This tool reports; deciding whether a capture
//! passed is TASK-031's job. The exception is this program failing to read its own
//! input — either the file could not be opened at all, or a read failed partway through
//! — because reporting on less than the whole capture is not the same as reporting a
//! clean one.

use std::env;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::exit;

use asperitas_logging::frame::{encode, Decoder, Record, Stats, MAX_FRAME};
use asperitas_logging::Level;

/// Bytes requested per read. Larger than [`MAX_FRAME`] on purpose: a reader that happens
/// to align with record boundaries proves nothing about the decoder's boundaries.
const CHUNK_BYTES: usize = 4096;

fn main() {
    let mut input = open_input();
    let mut decoder = Decoder::new();
    let mut sink = Emitter::new();
    let mut chunk = [0u8; CHUNK_BYTES];
    let mut frame_buf = [0u8; MAX_FRAME];

    // Every byte the decoder took is counted here rather than derived later, so the
    // accounting law below compares quantities that were each measured, not inferred.
    let mut pushed: u64 = 0;
    // Set when the input itself could not be read to the end. Kept apart from every
    // integrity counter, because those describe the capture and this describes this
    // program's own failure to look at all of it.
    let mut unread = false;

    loop {
        let filled = match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("console_decode: read failed: {e}");
                unread = true;
                break;
            }
        };

        // `push` takes only what the decoder can currently hold, so the remainder of the
        // chunk stays here until the records waiting inside it have been handed to the
        // sink. That handshake is what makes this loop lossless however long the read.
        let mut offset = 0usize;
        while offset < filled {
            let before = offset;
            offset += decoder.push(&chunk[offset..filled]);
            drain(&mut decoder, &mut sink, &mut frame_buf);
            if offset == before {
                // Unreachable while `next_record` is being drained: freeing a queue slot
                // always lets `push` take more input. Reported, not assumed.
                eprintln!("console_decode: decoder took no further bytes; stopping early");
                unread = true;
                break;
            }
        }
        // Count what the decoder actually took. Bytes still waiting in this chunk when we
        // stopped early were never offered, so they belong to no counter.
        pushed += offset as u64;
    }

    // Required before reading the counters: whatever is still undecided has nowhere to
    // come from, so it becomes discarded bytes rather than an invisible remainder.
    decoder.finish();
    drain(&mut decoder, &mut sink, &mut frame_buf);
    sink.close();

    report(
        decoder.stats(),
        pushed,
        sink.framed_bytes,
        decoder.buffered(),
        &sink.continuity,
    );

    // The summary is printed either way: the numbers are the point, and an operator who
    // loses them to a non-zero exit has lost the diagnosis too.
    if unread {
        exit(1);
    }
}

/// Hand every queued record to the sink, one at a time as the decoder allows.
fn drain(decoder: &mut Decoder, sink: &mut Emitter, frame_buf: &mut [u8; MAX_FRAME]) {
    while let Some(record) = decoder.next_record() {
        sink.emit(&record, frame_buf);
    }
}

/// Validated records, and what happens when nothing is listening to stdout any more.
///
/// A closed stdout (`console_decode capture.bin | head`) stops the printing but not the
/// counting: the run decodes to the end so the summary still describes the whole capture
/// instead of wherever `head` happened to stop. Halting both together would report the
/// unread remainder as loss, which is a number this program exists to get right.
struct Emitter {
    out: BufWriter<io::StdoutLock<'static>>,
    open: bool,
    framed_bytes: u64,
    continuity: Continuity,
}

impl Emitter {
    fn new() -> Self {
        Self {
            out: BufWriter::new(io::stdout().lock()),
            open: true,
            framed_bytes: 0,
            continuity: Continuity::default(),
        }
    }

    fn emit(&mut self, record: &Record<'_>, frame_buf: &mut [u8; MAX_FRAME]) {
        let bytes = reencoded_frame(record, frame_buf);
        self.framed_bytes += bytes.len() as u64;
        self.continuity.observe(record.seq);
        if !self.open {
            return;
        }
        if let Err(e) = self.out.write_all(bytes) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("console_decode: stdout write failed: {e}");
            }
            self.open = false;
        }
    }

    fn close(&mut self) {
        let _ = self.out.flush();
    }
}

/// Open the file named on the command line, or stdin when none was given.
///
/// A file that will not open exits non-zero — one of two cases this program decides for
/// itself, the other being a read that fails partway through (see `main`'s `unread`
/// handling). The distinction matters because the exit code means “this capture was read
/// in full and reported on”: contents — corrupt frames, truncation, garbage — still exit
/// 0, since whether such a capture passed is TASK-031's call. A typo'd path or a read
/// that dies early exiting 0 would let a scripted rig report success having decoded only
/// part of a capture, or none at all.
fn open_input() -> Box<dyn Read> {
    match env::args().nth(1) {
        None => Box::new(io::stdin()),
        Some(path) => match File::open(&path) {
            Ok(file) => Box::new(BufReader::new(file)),
            Err(e) => {
                eprintln!("console_decode: cannot read {path}: {e}");
                exit(1);
            }
        },
    }
}

/// Reassemble the wire bytes a validated record arrived as.
///
/// The record already passed CRC, so re-encoding its own fields and body is not
/// re-checking anything — it reproduces the frame byte for byte, because the prefix is
/// fixed-width, the body arrives pre-sanitised, and the checksum is a function of the
/// two. Printing that is what lets this output be grepped and piped back in.
fn reencoded_frame<'a>(record: &Record<'_>, buf: &'a mut [u8; MAX_FRAME]) -> &'a [u8] {
    let encoded = encode(
        level_from_letter(record.level),
        record.seq,
        record.t_ms,
        record.body,
        buf,
    );
    &buf[..encoded.len]
}

fn level_from_letter(letter: u8) -> Level {
    match letter {
        b'W' => Level::Warn,
        b'E' => Level::Error,
        b'D' => Level::Debug,
        b'T' => Level::Trace,
        // `I` is the only remaining wire letter; defaulting keeps this total rather than
        // panicking on a byte the decoder would already have rejected.
        _ => Level::Info,
    }
}

/// Sequence continuity across the records that validated.
#[derive(Default)]
struct Continuity {
    observed: u64,
    gaps: u64,
    first_gap: Option<(u32, u32)>,
    previous: Option<u32>,
}

impl Continuity {
    /// Note one record's sequence number. Consecutive numbering wraps at `u32::MAX` and
    /// the wrap is not a gap.
    fn observe(&mut self, seq: u32) {
        self.observed += 1;
        if let Some(previous) = self.previous {
            if seq != previous.wrapping_add(1) {
                self.gaps += 1;
                if self.first_gap.is_none() {
                    self.first_gap = Some((previous, seq));
                }
            }
        }
        self.previous = Some(seq);
    }
}

/// Print the four integrity counters, then show that they account for every byte.
///
/// The law is printed rather than merely checked because it is the reader's evidence:
/// `discarded_bytes = 0` means something only when the bytes you pushed are all present
/// somewhere in the summary.
fn report(stats: Stats, pushed: u64, framed_bytes: u64, buffered: usize, continuity: &Continuity) {
    let accounted = framed_bytes + stats.discarded_bytes + buffered as u64;
    eprintln!(
        "console_decode: records={} bad_frames={} resyncs={} discarded_bytes={}",
        stats.records, stats.bad_frames, stats.resyncs, stats.discarded_bytes
    );
    eprintln!(
        "console_decode: bytes pushed={} accounted_for={} (framed_records={} discarded={} buffered={})",
        pushed, accounted, framed_bytes, stats.discarded_bytes, buffered
    );
    if accounted != pushed {
        eprintln!(
            "console_decode: ACCOUNTING FAILURE: {} bytes are in neither category",
            pushed - accounted
        );
    }
    eprintln!(
        "console_decode: seq continuity: records={} gaps={} first_gap={}",
        continuity.observed,
        continuity.gaps,
        match continuity.first_gap {
            None => "none".to_string(),
            Some((from, to)) => format!("{from:08x}->{to:08x}"),
        }
    );
    eprintln!(
        "console_decode: note: CRC verifies integrity, never absence. A gap right after a BOOT \
         record is a restart, not loss."
    );
}
