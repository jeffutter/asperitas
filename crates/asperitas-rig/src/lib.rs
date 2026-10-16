//! Host-side capture engine for the Asperitas framed console.
//!
//! [`capture`] reads bytes from any [`io::Read`], validates them through
//! [`asperitas_wire::frame::Decoder`], and returns a [`Capture`]: the validated records
//! plus a [`Summary`] of everything the wire and the device reported. [`verdict`] turns a
//! summary into pass or fail, and [`render`] writes a deterministic text artifact that can
//! be committed and diffed.
//!
//! The loop mirrors `crates/asperitas-wire/examples/console_decode.rs` (about 40 lines
//! of chunking and the push/drain handshake). It is duplicated here deliberately: the
//! example is a binary and exposing its loop from `asperitas-wire` would add host-only
//! `std` API to a firmware crate.
//!
//! # What fails a capture
//!
//! * any bad frame, any discarded byte (including garbage before the first valid frame -
//!   loud on purpose), or an accounting mismatch;
//! * device-reported drops (`dropped_full` from `STATUS` records, summed per boot) above
//!   [`Policy::max_device_dropped`];
//! * no `STATUS` record at all, only when [`Policy::require_status`] is set.
//!
//! Sequence gaps are reported but do not fail on their own: a CRC cannot prove absence,
//! and a gap can be a restart. A gap at a `BOOT` record is counted in
//! [`Summary::restarts`], not [`Summary::seq_gaps`].
//!
//! A read error is returned as `Err` from [`capture`]; it is never folded into a verdict,
//! so a short read cannot be mistaken for a clean capture.

pub mod cli;
pub mod control;

use std::fmt::Write as _;
use std::io::{self, Read};

use asperitas_wire::frame::{Decoder, Record, MAX_FRAME};

/// Bytes requested per read; larger than one frame so boundaries never align by luck.
const CHUNK_BYTES: usize = 4096;

/// One validated record, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRecord {
    /// Wire level letter (`I`, `W`, `E`, `D`, `T`).
    pub level: char,
    /// Sequence number as transmitted.
    pub seq: u32,
    /// Device milliseconds since boot (modulo 1e8).
    pub t_ms: u32,
    /// Body, lossily converted to UTF-8 (the wire sanitises it to ASCII).
    pub body: String,
}

/// Counters describing a capture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Validated records.
    pub records: u64,
    /// Bytes handed to the decoder.
    pub bytes_pushed: u64,
    /// Candidate frames rejected.
    pub bad_frames: u64,
    /// Rejections followed by a later start marker.
    pub resyncs: u64,
    /// Bytes that never became a validated record.
    pub discarded_bytes: u64,
    /// True when pushed bytes == framed + discarded + buffered.
    pub accounting_ok: bool,
    /// Sequence discontinuities, excluding those at a `BOOT` record.
    pub seq_gaps: u64,
    /// First gap as `(previous, next)` sequence numbers.
    pub first_gap: Option<(u32, u32)>,
    /// `BOOT` records seen after the first record (device restarts mid-capture).
    pub restarts: u64,
    /// Parsed `STATUS` records.
    pub status_records: u64,
    /// `STATUS` records with an unknown protocol or unparseable fields.
    pub unparsed_status: u64,
    /// Device-reported `dropped_full`, summed over boots using the last `STATUS` of each.
    /// `None` when no `STATUS` record parsed.
    pub device_dropped: Option<u64>,
}

/// A finished capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    /// Validated records in arrival order.
    pub records: Vec<OwnedRecord>,
    /// Counters.
    pub summary: Summary,
}

/// Pass/fail thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Policy {
    /// Device-reported drops above this fail the capture. Default 0.
    pub max_device_dropped: u64,
    /// Fail when no `STATUS` record was seen (device drops unknowable).
    pub require_status: bool,
}

/// Why a capture failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Frames were rejected by the decoder.
    BadFrames(u64),
    /// Bytes were discarded outside any valid record.
    DiscardedBytes(u64),
    /// The byte-accounting law did not hold.
    AccountingMismatch,
    /// The device reported more drops than the policy allows.
    DeviceDropped { reported: u64, max: u64 },
    /// `require_status` was set and no `STATUS` record parsed.
    NoStatus,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::BadFrames(n) => write!(f, "bad_frames={n}"),
            Reason::DiscardedBytes(n) => write!(f, "discarded_bytes={n}"),
            Reason::AccountingMismatch => write!(f, "accounting_mismatch"),
            Reason::DeviceDropped { reported, max } => {
                write!(f, "device_dropped={reported} exceeds max={max}")
            }
            Reason::NoStatus => write!(f, "no_status_record"),
        }
    }
}

/// Outcome of [`verdict`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Capture is trustworthy.
    Pass,
    /// Capture is not; every reason is listed.
    Fail(Vec<Reason>),
}

impl Verdict {
    /// True for [`Verdict::Pass`].
    pub fn is_pass(&self) -> bool {
        matches!(self, Verdict::Pass)
    }
}

/// Decode everything `reader` yields.
///
/// Returns `Err` if the read fails partway (the partial capture is discarded rather than
/// reported as complete).
pub fn capture<R: Read>(mut reader: R) -> io::Result<Capture> {
    let mut decoder = Decoder::new();
    let mut acc = Accumulator::default();
    let mut chunk = [0u8; CHUNK_BYTES];
    let mut pushed: u64 = 0;

    loop {
        let filled = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        // `push` takes only what the decoder can hold; drain between pushes so the
        // remainder of the chunk is never lost.
        let mut offset = 0usize;
        while offset < filled {
            let taken = decoder.push(&chunk[offset..filled]);
            drain(&mut decoder, &mut acc);
            if taken == 0 {
                return Err(io::Error::other("decoder made no progress"));
            }
            offset += taken;
        }
        pushed += filled as u64;
    }

    decoder.finish();
    drain(&mut decoder, &mut acc);

    let stats = decoder.stats();
    let accounted = acc.framed_bytes + stats.discarded_bytes + decoder.buffered() as u64;
    let mut summary = acc.summary;
    summary.records = stats.records;
    summary.bytes_pushed = pushed;
    summary.bad_frames = stats.bad_frames;
    summary.resyncs = stats.resyncs;
    summary.discarded_bytes = stats.discarded_bytes;
    summary.accounting_ok = accounted == pushed;
    summary.device_dropped = acc
        .any_status
        .then(|| acc.dropped_total + acc.dropped_this_boot);
    Ok(Capture {
        records: acc.records,
        summary,
    })
}

#[derive(Default)]
struct Accumulator {
    records: Vec<OwnedRecord>,
    summary: Summary,
    framed_bytes: u64,
    previous_seq: Option<u32>,
    any_status: bool,
    dropped_total: u64,
    dropped_this_boot: u64,
}

fn drain(decoder: &mut Decoder, acc: &mut Accumulator) {
    while let Some(r) = decoder.next_record() {
        acc.observe(&r);
    }
}

impl Accumulator {
    fn observe(&mut self, r: &Record<'_>) {
        let body = String::from_utf8_lossy(r.body).into_owned();
        // Frame = '~' + prefix + body + crc + terminator; ask the codec rather than
        // hard-coding its layout.
        self.framed_bytes += framed_len(r) as u64;

        let is_boot = body.starts_with("BOOT ");
        if let Some(prev) = self.previous_seq {
            if is_boot {
                self.summary.restarts += 1;
            } else if r.seq != prev.wrapping_add(1) {
                self.summary.seq_gaps += 1;
                self.summary.first_gap.get_or_insert((prev, r.seq));
            }
        }
        self.previous_seq = Some(r.seq);

        if is_boot {
            // Device counters restart from zero at boot.
            self.dropped_total += self.dropped_this_boot;
            self.dropped_this_boot = 0;
        } else if body.starts_with("STATUS ") {
            match parse_status(&body) {
                Some(dropped) => {
                    self.summary.status_records += 1;
                    self.any_status = true;
                    self.dropped_this_boot = dropped;
                }
                None => self.summary.unparsed_status += 1,
            }
        }

        self.records.push(OwnedRecord {
            level: r.level as char,
            seq: r.seq,
            t_ms: r.t_ms,
            body,
        });
    }
}

fn framed_len(r: &Record<'_>) -> usize {
    let mut buf = [0u8; MAX_FRAME];
    let level = match r.level {
        b'W' => asperitas_wire::Level::Warn,
        b'E' => asperitas_wire::Level::Error,
        b'D' => asperitas_wire::Level::Debug,
        b'T' => asperitas_wire::Level::Trace,
        _ => asperitas_wire::Level::Info,
    };
    asperitas_wire::frame::encode(level, r.seq, r.t_ms, r.body, &mut buf).len
}

/// Parse `STATUS proto=1 ... dropped_full=N ...`; `None` for unknown proto or bad field.
fn parse_status(body: &str) -> Option<u64> {
    let mut proto_ok = false;
    let mut dropped = None;
    for tok in body.split_whitespace().skip(1) {
        match tok.split_once('=') {
            Some(("proto", v)) => proto_ok = v == "1",
            Some(("dropped_full", v)) => dropped = v.parse::<u64>().ok(),
            _ => {}
        }
    }
    if proto_ok {
        dropped
    } else {
        None
    }
}

/// Judge a summary against a policy.
pub fn verdict(summary: &Summary, policy: &Policy) -> Verdict {
    let mut reasons = Vec::new();
    if summary.bad_frames > 0 {
        reasons.push(Reason::BadFrames(summary.bad_frames));
    }
    if summary.discarded_bytes > 0 {
        reasons.push(Reason::DiscardedBytes(summary.discarded_bytes));
    }
    if !summary.accounting_ok {
        reasons.push(Reason::AccountingMismatch);
    }
    match summary.device_dropped {
        Some(d) if d > policy.max_device_dropped => reasons.push(Reason::DeviceDropped {
            reported: d,
            max: policy.max_device_dropped,
        }),
        None if policy.require_status => reasons.push(Reason::NoStatus),
        _ => {}
    }
    if reasons.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Fail(reasons)
    }
}

/// Render a deterministic, line-oriented artifact: header, `key=value` summary lines in
/// fixed order, a verdict line, then one line per record (`level seq t_ms body`).
///
/// Contains no wall-clock time, host path or duration, so the same wire bytes always
/// produce the same string.
pub fn render(capture: &Capture, policy: &Policy) -> String {
    let s = &capture.summary;
    let mut out = String::new();
    let _ = writeln!(out, "asperitas-rig capture v1");
    let _ = writeln!(out, "records={}", s.records);
    let _ = writeln!(out, "bytes_pushed={}", s.bytes_pushed);
    let _ = writeln!(out, "bad_frames={}", s.bad_frames);
    let _ = writeln!(out, "resyncs={}", s.resyncs);
    let _ = writeln!(out, "discarded_bytes={}", s.discarded_bytes);
    let _ = writeln!(out, "accounting_ok={}", s.accounting_ok);
    let _ = writeln!(out, "seq_gaps={}", s.seq_gaps);
    match s.first_gap {
        None => {
            let _ = writeln!(out, "first_gap=none");
        }
        Some((a, b)) => {
            let _ = writeln!(out, "first_gap={a:08x}->{b:08x}");
        }
    }
    let _ = writeln!(out, "restarts={}", s.restarts);
    let _ = writeln!(out, "status_records={}", s.status_records);
    let _ = writeln!(out, "unparsed_status={}", s.unparsed_status);
    match s.device_dropped {
        None => {
            let _ = writeln!(out, "device_dropped=unknown");
        }
        Some(d) => {
            let _ = writeln!(out, "device_dropped={d}");
        }
    }
    match verdict(s, policy) {
        Verdict::Pass => {
            let _ = writeln!(out, "verdict=pass");
        }
        Verdict::Fail(reasons) => {
            let list: Vec<String> = reasons.iter().map(ToString::to_string).collect();
            let _ = writeln!(out, "verdict=fail {}", list.join("; "));
        }
    }
    let _ = writeln!(out, "--");
    for r in &capture.records {
        let _ = writeln!(out, "{} {:08x} {} {}", r.level, r.seq, r.t_ms, r.body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_wire::frame::encode;

    fn frame(seq: u32, body: &str) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let e = encode(
            asperitas_wire::Level::Info,
            seq,
            seq.wrapping_mul(10),
            body.as_bytes(),
            &mut buf,
        );
        buf[..e.len].to_vec()
    }

    fn stream(items: &[(u32, &str)]) -> Vec<u8> {
        items.iter().flat_map(|(s, b)| frame(*s, b)).collect()
    }

    fn run(bytes: &[u8]) -> Capture {
        capture(bytes).unwrap()
    }

    fn status(dropped: u32) -> String {
        format!(
            "STATUS proto=1 sent=5 dropped_full={dropped} bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100"
        )
    }

    #[test]
    fn clean_stream_passes() {
        let st = status(0);
        let c = run(&stream(&[(0, "BOOT proto=1 fw=x"), (1, "hi"), (2, &st)]));
        assert_eq!(c.summary.records, 3);
        assert!(c.summary.accounting_ok);
        assert_eq!(c.summary.device_dropped, Some(0));
        assert_eq!(c.summary.seq_gaps, 0);
        assert!(verdict(&c.summary, &Policy::default()).is_pass());
    }

    #[test]
    fn truncated_final_frame_fails() {
        let mut b = stream(&[(0, "a"), (1, "b")]);
        let last = frame(2, "cccc");
        b.extend_from_slice(&last[..last.len() - 3]);
        let c = run(&b);
        assert_eq!(c.summary.records, 2);
        assert!(c.summary.discarded_bytes > 0);
        assert!(c.summary.accounting_ok);
        assert!(!verdict(&c.summary, &Policy::default()).is_pass());
    }

    #[test]
    fn corrupted_body_is_bad_frame() {
        let mut b = stream(&[(0, "a"), (1, "bbbbbb"), (2, "c")]);
        let first = frame(0, "a").len();
        b[first + 25] ^= 0x01;
        let c = run(&b);
        assert!(c.summary.bad_frames > 0);
        assert!(c.summary.accounting_ok);
        match verdict(&c.summary, &Policy::default()) {
            Verdict::Fail(r) => assert!(r.iter().any(|x| matches!(x, Reason::BadFrames(_)))),
            Verdict::Pass => panic!("should fail"),
        }
        // The record after the corruption is still recovered.
        assert!(c.records.iter().any(|r| r.body == "c"));
    }

    #[test]
    fn garbage_prefix_is_discarded_and_fails() {
        let mut b = b"xyz\x00\xff".to_vec();
        b.extend(stream(&[(0, "a")]));
        let c = run(&b);
        assert_eq!(c.summary.records, 1);
        assert_eq!(c.summary.discarded_bytes, 5);
        assert!(!verdict(&c.summary, &Policy::default()).is_pass());
    }

    #[test]
    fn seq_gap_wrap_and_restart() {
        let c = run(&stream(&[
            (u32::MAX - 1, "a"),
            (u32::MAX, "b"),
            (0, "c"),
            (5, "d"),
            (0, "BOOT proto=1"),
            (1, "e"),
        ]));
        assert_eq!(c.summary.seq_gaps, 1);
        assert_eq!(c.summary.first_gap, Some((0, 5)));
        assert_eq!(c.summary.restarts, 1);
    }

    #[test]
    fn device_drop_threshold_both_sides() {
        let s3 = status(3);
        let c = run(&stream(&[(0, "x"), (1, &s3)]));
        assert_eq!(c.summary.device_dropped, Some(3));
        let at = Policy {
            max_device_dropped: 3,
            ..Policy::default()
        };
        let below = Policy {
            max_device_dropped: 2,
            ..Policy::default()
        };
        assert!(verdict(&c.summary, &at).is_pass());
        assert_eq!(
            verdict(&c.summary, &below),
            Verdict::Fail(vec![Reason::DeviceDropped {
                reported: 3,
                max: 2
            }])
        );
    }

    #[test]
    fn drops_are_summed_per_boot_using_last_status() {
        let (a, b, c2) = (status(2), status(4), status(1));
        let c = run(&stream(&[
            (0, "BOOT proto=1"),
            (1, &a),
            (2, &b),
            (0, "BOOT proto=1"),
            (1, &c2),
        ]));
        assert_eq!(c.summary.device_dropped, Some(5));
    }

    #[test]
    fn missing_status_and_unknown_proto() {
        let c = run(&stream(&[(0, "x"), (1, "STATUS proto=2 dropped_full=9")]));
        assert_eq!(c.summary.device_dropped, None);
        assert_eq!(c.summary.unparsed_status, 1);
        assert!(verdict(&c.summary, &Policy::default()).is_pass());
        let strict = Policy {
            require_status: true,
            ..Policy::default()
        };
        assert_eq!(
            verdict(&c.summary, &strict),
            Verdict::Fail(vec![Reason::NoStatus])
        );
    }

    struct OneByte<'a>(&'a [u8]);
    impl Read for OneByte<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.0.split_first() {
                Some((b, rest)) if !buf.is_empty() => {
                    buf[0] = *b;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    #[test]
    fn chunking_does_not_change_artifact() {
        let st = status(1);
        let mut b = stream(&[(0, "BOOT proto=1"), (1, "hello"), (2, &st)]);
        b.extend_from_slice(&frame(3, "tail")[..7]);
        let p = Policy::default();
        let whole = render(&run(&b), &p);
        let single = render(&capture(OneByte(&b)).unwrap(), &p);
        assert_eq!(whole, single);
    }

    #[test]
    fn artifact_golden() {
        let c = run(&stream(&[(0, "BOOT proto=1"), (1, "hi")]));
        let got = render(&c, &Policy::default());
        let first = frame(0, "BOOT proto=1").len();
        let second = frame(1, "hi").len();
        let expected = format!(
            "asperitas-rig capture v1\nrecords=2\nbytes_pushed={}\nbad_frames=0\nresyncs=0\n\
             discarded_bytes=0\naccounting_ok=true\nseq_gaps=0\nfirst_gap=none\nrestarts=0\n\
             status_records=0\nunparsed_status=0\ndevice_dropped=unknown\nverdict=pass\n--\n\
             I 00000000 0 BOOT proto=1\nI 00000001 10 hi\n",
            first + second
        );
        assert_eq!(got, expected);
    }

    struct Failing(usize);
    impl Read for Failing {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "unplugged"));
            }
            self.0 -= 1;
            let f = frame(0, "x");
            buf[..f.len()].copy_from_slice(&f);
            Ok(f.len())
        }
    }

    #[test]
    fn read_error_is_err_not_verdict() {
        assert!(capture(Failing(2)).is_err());
    }

    #[test]
    fn accounting_holds_under_random_corruption() {
        let base = stream(&(0..40).map(|i| (i, "payload text")).collect::<Vec<_>>());
        let mut state: u64 = 0x1234_5678_9abc_def1;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };
        for _ in 0..200 {
            let mut b = base.clone();
            for _ in 0..(next() % 8) {
                let i = next() % b.len();
                b[i] = next() as u8;
            }
            let cut = b.len() - next() % 20;
            b.truncate(cut);
            let c = run(&b);
            assert!(c.summary.accounting_ok);
        }
    }
}
