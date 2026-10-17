//! `console_decode` — turn a raw USB console capture into validated records plus a
//! capture summary whose numbers add up.
//!
//! Run it on a file, or pipe a capture through it:
//!
//! ```sh
//! cargo run -p asperitas-wire --example console_decode -- capture.bin > clean.txt
//! cat capture.bin | cargo run -p asperitas-wire --example console_decode
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

use asperitas_wire::console::Continuity;
use asperitas_wire::frame::{encode, Decoder, Record, Stats, MAX_FRAME};

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

        // `feed` is the lossless push/drain loop: the remainder of the chunk waits until the
        // records inside it have been handed to the sink.
        let taken = decoder.feed(&chunk[..filled], |record| {
            sink.emit(record, &mut frame_buf);
        });
        // Count what the decoder actually took. Bytes still waiting in this chunk when we
        // stopped early were never offered, so they belong to no counter.
        pushed += taken as u64;
        if taken < filled {
            // Unreachable while records are being drained: freeing a queue slot always lets
            // `push` take more input. Reported, not assumed.
            eprintln!("console_decode: decoder took no further bytes; stopping early");
            unread = true;
            break;
        }
    }

    // Required before reading the counters: whatever is still undecided has nowhere to
    // come from, so it becomes discarded bytes rather than an invisible remainder.
    decoder.finish();
    sink.close();

    report(&decoder, pushed, &sink.continuity);

    // The summary is printed either way: the numbers are the point, and an operator who
    // loses them to a non-zero exit has lost the diagnosis too.
    if unread {
        exit(1);
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
    continuity: Continuity,
}

impl Emitter {
    fn new() -> Self {
        Self {
            out: BufWriter::new(io::stdout().lock()),
            open: true,
            continuity: Continuity::default(),
        }
    }

    fn emit(&mut self, record: &Record<'_>, frame_buf: &mut [u8; MAX_FRAME]) {
        let bytes = reencoded_frame(record, frame_buf);
        self.continuity.observe(record);
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
    let encoded = encode(record.level(), record.seq, record.t_ms, record.body, buf);
    &buf[..encoded.len]
}

/// Print the four integrity counters, then show that they account for every byte.
///
/// The law is printed rather than merely checked because it is the reader's evidence:
/// `discarded_bytes = 0` means something only when the bytes you pushed are all present
/// somewhere in the summary.
fn report(decoder: &Decoder, pushed: u64, continuity: &Continuity) {
    let stats: Stats = decoder.stats();
    let buffered = decoder.buffered();
    let accounted = stats.framed_bytes + stats.discarded_bytes + buffered as u64;
    eprintln!(
        "console_decode: records={} bad_frames={} resyncs={} discarded_bytes={}",
        stats.records, stats.bad_frames, stats.resyncs, stats.discarded_bytes
    );
    eprintln!(
        "console_decode: bytes pushed={} accounted_for={} (framed_records={} discarded={} buffered={})",
        pushed, accounted, stats.framed_bytes, stats.discarded_bytes, buffered
    );
    if !decoder.accounted(pushed) {
        eprintln!(
            "console_decode: ACCOUNTING FAILURE: {} bytes are in neither category",
            pushed.abs_diff(accounted)
        );
    }
    eprintln!(
        "console_decode: seq continuity: records={} gaps={} restarts={} first_gap={}",
        continuity.observed,
        continuity.gaps,
        continuity.restarts,
        match continuity.first_gap {
            None => "none".to_string(),
            Some((from, to)) => format!("{from:08x}->{to:08x}"),
        }
    );
    eprintln!(
        "console_decode: note: CRC verifies integrity, never absence. A BOOT record restarts \
         the numbering and counts as a restart, not a gap."
    );
}
