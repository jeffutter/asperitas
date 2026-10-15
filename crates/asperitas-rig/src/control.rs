//! Host side of the device control channel: send [`Command`]s as ordinary v1 frames on the
//! same port the rig captures from, and match replies in the stream.
//!
//! Grammar and codec live in [`asperitas_logging::control`]; nothing about the wire is
//! restated here. Everything is generic over `Read + Write` so tests drive it with an
//! in-memory fake and `main.rs` only opens the port.
//!
//! Two guarantees callers rely on:
//!
//! * a device `CTLR ERR` is surfaced as [`ControlError::Rejected`] (never swallowed), and
//!   rejected or corrupt frames seen while waiting are counted in [`Link::bad_frames`];
//! * an override sweep always sends `REL` for its knob, on success, on a capture error and
//!   on a write error alike ([`run_override`]).

use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use asperitas_logging::control::{parse_reply, BodyBuf, Command, ErrorCode, Reply, Verb};
use asperitas_logging::frame::{encode, Decoder, MAX_FRAME};
use asperitas_logging::Level;

use crate::cli::{report, Deadline, Outcome};
use crate::{capture, Policy};

/// Step rate of a sweep, in `SET` commands per second.
const SWEEP_HZ: f64 = 20.0;
/// How long after the last command the capture keeps listening for the final acks.
const ACK_GRACE: Duration = Duration::from_millis(500);

/// Why a control operation did not complete.
#[derive(Debug)]
pub enum ControlError {
    /// Read or write failure.
    Io(io::Error),
    /// The port reported end of file.
    Closed,
    /// No matching reply arrived in time.
    Timeout,
    /// The device answered `CTLR ERR`.
    Rejected(ErrorCode),
    /// A value outside its legal range (knob level not in `0..=1`, bad sweep).
    Usage(String),
    /// The console never came back after a restart.
    NoDevice(String),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::Io(e) => write!(f, "i/o error: {e}"),
            ControlError::Closed => write!(f, "port closed"),
            ControlError::Timeout => write!(f, "timed out waiting for a device reply"),
            ControlError::Rejected(c) => write!(f, "device rejected the command: {c:?}"),
            ControlError::Usage(s) | ControlError::NoDevice(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ControlError {}

impl From<io::Error> for ControlError {
    fn from(e: io::Error) -> Self {
        ControlError::Io(e)
    }
}

impl ControlError {
    /// Exit status: a device rejection is a failed result, everything else is an error.
    pub fn outcome(&self) -> Outcome {
        match self {
            ControlError::Rejected(_) => Outcome::Fail,
            _ => Outcome::Error,
        }
    }
}

/// Wrap a command in a v1 frame. Host frames carry the command counter as `seq` and no clock.
pub fn frame_command(cmd: Command, seq: u32, out: &mut [u8; MAX_FRAME]) -> usize {
    let mut body: BodyBuf = [0; asperitas_logging::frame::MAX_BODY];
    let n = cmd.encode(&mut body);
    encode(Level::Info, seq, 0, &body[..n], out).len
}

/// A request/reply channel over a bidirectional byte stream.
pub struct Link<P> {
    port: P,
    decoder: Decoder,
    seq: u32,
    timeout: Duration,
    saw_boot: bool,
}

impl<P: Read + Write> Link<P> {
    /// `timeout` bounds each wait for a reply; the port should have a short read timeout.
    pub fn new(port: P, timeout: Duration) -> Self {
        Self {
            port,
            decoder: Decoder::new(),
            seq: 0,
            timeout,
            saw_boot: false,
        }
    }

    /// Candidate frames rejected so far while waiting for replies.
    pub fn bad_frames(&self) -> u64 {
        self.decoder.stats().bad_frames
    }

    /// True once a `BOOT` record has passed through while waiting.
    pub fn saw_boot(&self) -> bool {
        self.saw_boot
    }

    /// Send `cmd` and wait for its reply: the value reply for `VER`/`RSN`, `OK <verb>`
    /// for the rest. Log records and unrelated replies in between are skipped.
    pub fn request(&mut self, cmd: Command) -> Result<Reply, ControlError> {
        let mut frame = [0u8; MAX_FRAME];
        let n = frame_command(cmd, self.seq, &mut frame);
        self.seq = self.seq.wrapping_add(1);
        self.port.write_all(&frame[..n])?;
        self.port.flush()?;
        let want = cmd.verb();
        let reply = self.wait(|body| match parse_reply(body.as_bytes()) {
            Ok(r @ Reply::Version(_)) if want == Verb::Version => Some(r),
            Ok(r @ Reply::ResetReason(_)) if want == Verb::ResetReason => Some(r),
            Ok(r @ Reply::Ok(v)) if v == want => Some(r),
            Ok(r @ Reply::Error(_)) => Some(r),
            _ => None,
        })?;
        match reply {
            Reply::Error(code) => Err(ControlError::Rejected(code)),
            r => Ok(r),
        }
    }

    /// Firmware version text.
    pub fn version(&mut self) -> Result<String, ControlError> {
        match self.request(Command::GetVersion)? {
            Reply::Version(t) => Ok(String::from_utf8_lossy(t.as_bytes()).into_owned()),
            _ => unreachable!("request filters replies by verb"),
        }
    }

    /// Last reset reason token.
    pub fn reset_reason(&mut self) -> Result<String, ControlError> {
        match self.request(Command::GetResetReason)? {
            Reply::ResetReason(t) => Ok(String::from_utf8_lossy(t.as_bytes()).into_owned()),
            _ => unreachable!("request filters replies by verb"),
        }
    }

    /// Read until `pick` accepts a record body or `timeout` passes. Bytes in the same read
    /// chunk after the accepted record are dropped; callers wait on one reply at a time.
    fn wait<T>(&mut self, mut pick: impl FnMut(&str) -> Option<T>) -> Result<T, ControlError> {
        let deadline = Instant::now() + self.timeout;
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(v) = self.drain(&mut pick) {
                return Ok(v);
            }
            if Instant::now() >= deadline {
                return Err(ControlError::Timeout);
            }
            let n = match self.port.read(&mut chunk) {
                Ok(0) => return Err(ControlError::Closed),
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            };
            let mut off = 0;
            while off < n {
                let taken = self.decoder.push(&chunk[off..n]);
                if let Some(v) = self.drain(&mut pick) {
                    return Ok(v);
                }
                if taken == 0 {
                    return Err(ControlError::Io(io::Error::other(
                        "decoder made no progress",
                    )));
                }
                off += taken;
            }
        }
    }

    fn drain<T>(&mut self, pick: &mut impl FnMut(&str) -> Option<T>) -> Option<T> {
        while let Some(r) = self.decoder.next_record() {
            let body = String::from_utf8_lossy(r.body);
            if body.starts_with("BOOT ") {
                self.saw_boot = true;
            }
            if let Some(v) = pick(&body) {
                return Some(v);
            }
        }
        None
    }
}

/// Firmware version and reset reason, as `query` prints them.
#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    /// Firmware version text.
    pub fw: String,
    /// Reset-reason token.
    pub reset_reason: String,
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fw={} reset_reason={}", self.fw, self.reset_reason)
    }
}

/// Ask for version then reset reason.
pub fn query<P: Read + Write>(link: &mut Link<P>) -> Result<Identity, ControlError> {
    Ok(Identity {
        fw: link.version()?,
        reset_reason: link.reset_reason()?,
    })
}

/// Timing for [`restart`]. Counted in polls so tests need no clock.
#[derive(Clone, Copy, Debug)]
pub struct RestartOpts {
    /// Give up waiting for the console to come back after this long.
    pub reenumerate_timeout: Duration,
    /// Delay between device-list polls.
    pub poll: Duration,
}

/// Result of a successful [`restart`].
#[derive(Debug, PartialEq, Eq)]
pub struct Restarted {
    /// Port the new console appeared on (the USB number can change).
    pub path: PathBuf,
    /// Identity reported by the new boot.
    pub identity: Identity,
    /// Whether the `BOOT` record was seen before the query reply. The device may log it
    /// before the host attaches, so absence is reported, not fatal; the query reply is
    /// the proof of life.
    pub boot_record: bool,
}

/// Request a restart, wait for the console to vanish and re-enumerate, reopen it, and
/// query the new boot.
///
/// The vanish phase is bounded: a fast reboot can come and go between polls, so when the
/// port is still listed after a short grace the code proceeds to reopen anyway and lets
/// the post-restart query decide.
pub fn restart<P, L, O, S>(
    mut link: Link<P>,
    mut list: L,
    mut open: O,
    mut sleep: S,
    opts: RestartOpts,
) -> Result<Restarted, ControlError>
where
    P: Read + Write,
    L: FnMut() -> io::Result<Vec<PathBuf>>,
    O: FnMut(&Path) -> io::Result<P>,
    S: FnMut(Duration),
{
    link.request(Command::Restart)?;
    let timeout = link.timeout;
    drop(link);

    let poll_ms = opts.poll.as_millis().max(1);
    let max_polls = (opts.reenumerate_timeout.as_millis() / poll_ms).max(1) as u64;
    let vanish_polls = max_polls.min(2_000 / poll_ms as u64 + 1);

    let mut polls = 0u64;
    while polls < vanish_polls && !list()?.is_empty() {
        polls += 1;
        sleep(opts.poll);
    }
    loop {
        let found = list()?;
        match found.len() {
            0 => {}
            1 => {
                if let Ok(port) = open(&found[0]) {
                    let mut new = Link::new(port, timeout);
                    let identity = query(&mut new)?;
                    return Ok(Restarted {
                        path: found[0].clone(),
                        identity,
                        boot_record: new.saw_boot(),
                    });
                }
            }
            _ => {
                return Err(ControlError::NoDevice(
                    crate::cli::DeviceError::Ambiguous(found).to_string(),
                ))
            }
        }
        polls += 1;
        if polls >= max_polls + vanish_polls {
            return Err(ControlError::NoDevice(format!(
                "console did not come back within {:.1} s of the restart",
                opts.reenumerate_timeout.as_secs_f64()
            )));
        }
        sleep(opts.poll);
    }
}

/// Convert a level in `0..=1` to the wire value `0x0000..=0xFFFF`. Out of range is an
/// error, never clamped: a clamped sweep would silently test something else.
pub fn level_to_wire(x: f64) -> Result<u16, ControlError> {
    if !(0.0..=1.0).contains(&x) {
        return Err(ControlError::Usage(format!(
            "knob level {x} is outside 0..=1"
        )));
    }
    Ok((x * f64::from(u16::MAX)).round() as u16)
}

/// `SET` schedule for a hold (`from == to`) or a sweep: `(offset from start, wire value)`.
///
/// A hold is one `SET` at time zero. A sweep steps at [`SWEEP_HZ`], reaching `to` on its
/// last step.
pub fn schedule(
    from: f64,
    to: f64,
    duration: Duration,
) -> Result<Vec<(Duration, u16)>, ControlError> {
    let (a, b) = (level_to_wire(from)?, level_to_wire(to)?);
    if a == b {
        return Ok(vec![(Duration::ZERO, a)]);
    }
    let steps = ((duration.as_secs_f64() * SWEEP_HZ).ceil() as usize).max(2);
    Ok((0..steps)
        .map(|i| {
            let frac = i as f64 / (steps - 1) as f64;
            let t = duration.mul_f64(i as f64 / steps as f64);
            (t, level_to_wire(from + (to - from) * frac).unwrap_or(b))
        })
        .collect())
}

/// What to override and for how long.
#[derive(Clone, Debug)]
pub struct OverridePlan {
    /// Knob id on the wire.
    pub knob: u8,
    /// Start level, `0..=1`.
    pub from: f64,
    /// End level, `0..=1`; equal to `from` for a hold.
    pub to: f64,
    /// Capture and sweep length.
    pub duration: Duration,
}

/// Run `plan` while capturing `reader`; write the artifact; always release the knob.
///
/// The capture runs on its own thread against `reader` and ends `ACK_GRACE` after the
/// sweep, so the final `REL` ack is inside it. The calling thread owns `writer`: it sends
/// the `SET`s on schedule, then `REL` whatever happened - a finished sweep, a capture that
/// died, or a failed write. Acks are read back from the capture: every `SET` and the `REL`
/// must be acknowledged and no `CTLR ERR` may appear, on top of the normal capture verdict.
pub fn run_override<R, W>(
    reader: R,
    mut writer: W,
    plan: &OverridePlan,
    policy: &Policy,
    out: &Path,
    stdout: &mut dyn Write,
) -> Result<Outcome, ControlError>
where
    R: Read + Send,
    W: Write,
{
    let sets = schedule(plan.from, plan.to, plan.duration)?;
    let start = Instant::now();
    let done = AtomicBool::new(false);

    let (cap, write_err) = std::thread::scope(|s| {
        let done = &done;
        let handle = s.spawn(move || {
            let r = capture(Deadline::new(reader, start + plan.duration + ACK_GRACE));
            done.store(true, Ordering::SeqCst);
            r
        });

        let mut seq = 0u32;
        let mut send = |cmd: Command| -> io::Result<()> {
            let mut frame = [0u8; MAX_FRAME];
            let n = frame_command(cmd, seq, &mut frame);
            seq = seq.wrapping_add(1);
            writer.write_all(&frame[..n])?;
            writer.flush()
        };
        let sleep_until = |offset: Duration| {
            while !done.load(Ordering::SeqCst) {
                let now = start.elapsed();
                if now >= offset {
                    break;
                }
                std::thread::sleep((offset - now).min(Duration::from_millis(10)));
            }
        };

        let mut write_err = None;
        for &(offset, value) in &sets {
            sleep_until(offset);
            if done.load(Ordering::SeqCst) {
                break;
            }
            let cmd = Command::OverrideSet {
                knob: plan.knob,
                value,
            };
            if let Err(e) = send(cmd) {
                write_err = Some(e);
                break;
            }
        }
        if write_err.is_none() {
            sleep_until(plan.duration);
        }
        let release = send(Command::OverrideRelease { knob: plan.knob });
        let write_err = write_err.or(release.err());
        let cap = handle
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("capture thread panicked")));
        (cap, write_err)
    });

    let cap = cap?;
    let capture_outcome = report(&cap, policy, out, stdout)?;

    let (mut set_acks, mut rel_acks, mut errors) = (0usize, 0usize, 0usize);
    for r in &cap.records {
        match parse_reply(r.body.as_bytes()) {
            Ok(Reply::Ok(Verb::Set)) => set_acks += 1,
            Ok(Reply::Ok(Verb::Release)) => rel_acks += 1,
            Ok(Reply::Error(_)) => errors += 1,
            _ => {}
        }
    }
    writeln!(
        stdout,
        "override knob={} sets_sent={} set_acks={set_acks} rel_acks={rel_acks} device_errors={errors}",
        plan.knob,
        sets.len()
    )?;
    if let Some(e) = write_err {
        return Err(ControlError::Io(e));
    }
    let acked = set_acks == sets.len() && rel_acks >= 1 && errors == 0;
    if !acked {
        writeln!(
            stdout,
            "override acks incomplete or device rejected a command"
        )?;
    }
    Ok(if acked && capture_outcome == Outcome::Pass {
        Outcome::Pass
    } else {
        Outcome::Fail
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_logging::control::{parse_command, ReasonText, VersionText};
    use asperitas_logging::frame::MAX_BODY;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn frame_body(seq: u32, body: &[u8]) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let e = encode(Level::Info, seq, seq * 10, body, &mut buf);
        buf[..e.len].to_vec()
    }

    fn reply_frame(seq: u32, r: Reply) -> Vec<u8> {
        let mut b = [0u8; MAX_BODY];
        let n = r.encode(&mut b);
        frame_body(seq, &b[..n])
    }

    fn ver(s: &str) -> Reply {
        Reply::Version(VersionText::version(s.as_bytes()).unwrap())
    }
    fn rsn(s: &str) -> Reply {
        Reply::ResetReason(ReasonText::reason(s.as_bytes()).unwrap())
    }

    /// Decode every command frame written to the shared buffer.
    fn written_commands(w: &Arc<Mutex<Vec<u8>>>) -> Vec<Command> {
        let mut d = Decoder::new();
        let bytes = w.lock().unwrap().clone();
        let mut out = Vec::new();
        let mut off = 0;
        while off < bytes.len() {
            off += d.push(&bytes[off..]);
            while let Some(r) = d.next_record() {
                out.push(parse_command(r.body).expect("valid command"));
            }
        }
        out
    }

    /// Replies are queued per write: each `write_all` consumes the next scripted chunk.
    struct Fake {
        script: VecDeque<Vec<u8>>,
        ready: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
    }
    impl Fake {
        fn new(script: Vec<Vec<u8>>) -> (Self, Arc<Mutex<Vec<u8>>>) {
            let written = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    script: script.into(),
                    ready: Vec::new(),
                    written: written.clone(),
                },
                written,
            )
        }
    }
    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.ready.is_empty() {
                std::thread::sleep(Duration::from_millis(1));
                return Err(io::ErrorKind::TimedOut.into());
            }
            let n = buf.len().min(self.ready.len());
            buf[..n].copy_from_slice(&self.ready[..n]);
            self.ready.drain(..n);
            Ok(n)
        }
    }
    impl Write for Fake {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.written.lock().unwrap().extend_from_slice(b);
            if let Some(next) = self.script.pop_front() {
                self.ready.extend(next);
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    const T: Duration = Duration::from_millis(200);

    #[test]
    fn command_frames_round_trip() {
        let cmds = [
            Command::GetVersion,
            Command::GetResetReason,
            Command::Restart,
            Command::OverrideSet {
                knob: 3,
                value: 0xabcd,
            },
            Command::OverrideRelease { knob: 3 },
        ];
        for c in cmds {
            let mut f = [0u8; MAX_FRAME];
            let n = frame_command(c, 7, &mut f);
            let mut d = Decoder::new();
            d.push(&f[..n]);
            let r = d.next_record().unwrap();
            assert_eq!(r.seq, 7);
            assert_eq!(parse_command(r.body), Ok(c));
        }
        let mut f = [0u8; MAX_FRAME];
        let n = frame_command(
            Command::OverrideSet {
                knob: 1,
                value: 0x0fff,
            },
            0,
            &mut f,
        );
        let mut d = Decoder::new();
        d.push(&f[..n]);
        assert_eq!(d.next_record().unwrap().body, b"CTL SET k=01 v=0fff");
    }

    #[test]
    fn query_skips_interleaved_logs() {
        let mut a = frame_body(0, b"knob a=1");
        a.extend(frame_body(1, b"CTLR OK RSN"));
        a.extend(reply_frame(2, ver("0.3.1")));
        let mut b = frame_body(3, b"noise");
        b.extend(reply_frame(4, rsn("software")));
        let (fake, written) = Fake::new(vec![a, b]);
        let mut link = Link::new(fake, T);
        let id = query(&mut link).unwrap();
        assert_eq!(id.to_string(), "fw=0.3.1 reset_reason=software");
        assert_eq!(link.bad_frames(), 0);
        assert_eq!(
            written_commands(&written),
            [Command::GetVersion, Command::GetResetReason]
        );
    }

    #[test]
    fn error_reply_is_rejected_and_fails() {
        let (fake, _) = Fake::new(vec![reply_frame(0, Reply::Error(ErrorCode::BadKnob))]);
        let mut link = Link::new(fake, T);
        let e = link
            .request(Command::OverrideSet { knob: 99, value: 0 })
            .unwrap_err();
        assert!(matches!(e, ControlError::Rejected(ErrorCode::BadKnob)));
        assert_eq!(e.outcome(), Outcome::Fail);
    }

    #[test]
    fn silence_times_out_as_error() {
        let (fake, _) = Fake::new(vec![]);
        let mut link = Link::new(fake, Duration::from_millis(30));
        let e = link.version().unwrap_err();
        assert!(matches!(e, ControlError::Timeout));
        assert_eq!(e.outcome(), Outcome::Error);
    }

    #[test]
    fn corrupt_frames_are_counted() {
        let mut bad = frame_body(0, b"some log line here");
        bad[22] ^= 1;
        let mut a = bad;
        a.extend(reply_frame(1, ver("1")));
        let (fake, _) = Fake::new(vec![a]);
        let mut link = Link::new(fake, T);
        link.version().unwrap();
        assert!(link.bad_frames() > 0);
    }

    #[test]
    fn schedule_hold_and_sweep() {
        assert_eq!(
            schedule(0.5, 0.5, Duration::from_secs(2)).unwrap(),
            [(Duration::ZERO, 0x8000)]
        );
        let s = schedule(0.0, 1.0, Duration::from_secs(1)).unwrap();
        assert_eq!(s.first().unwrap().1, 0);
        assert_eq!(s.last().unwrap().1, 0xffff);
        assert!(s.windows(2).all(|w| w[0].1 < w[1].1 && w[0].0 < w[1].0));
        assert!(s.last().unwrap().0 < Duration::from_secs(1));
        let down = schedule(1.0, 0.0, Duration::from_secs(1)).unwrap();
        assert!(down.windows(2).all(|w| w[0].1 > w[1].1));
        assert!(schedule(0.0, 1.5, Duration::from_secs(1)).is_err());
        assert!(schedule(-0.1, 0.5, Duration::from_secs(1)).is_err());
        assert!(schedule(f64::NAN, 0.5, Duration::from_secs(1)).is_err());
    }

    fn status_frame(seq: u32) -> Vec<u8> {
        frame_body(
            seq,
            b"STATUS proto=1 sent=5 dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100",
        )
    }

    /// A reader that acks every command the writer sends, then idles.
    struct Acker {
        written: Arc<Mutex<Vec<u8>>>,
        sent: usize,
        fail_after: Option<usize>,
        seq: u32,
        pending: Vec<u8>,
    }
    impl Read for Acker {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pending.is_empty() {
                let cmds = written_commands(&self.written);
                if self.fail_after.is_some_and(|n| cmds.len() > n) {
                    return Err(io::Error::other("unplugged"));
                }
                for c in &cmds[self.sent..] {
                    let r = Reply::Ok(c.verb());
                    self.pending.extend(reply_frame(self.seq, r));
                    self.seq += 1;
                }
                self.sent = cmds.len();
                if self.pending.is_empty() && self.seq == 0 {
                    self.pending.extend(status_frame(self.seq));
                    self.seq += 1;
                }
            }
            if self.pending.is_empty() {
                std::thread::sleep(Duration::from_millis(2));
                return Err(io::ErrorKind::TimedOut.into());
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    struct Tee(Arc<Mutex<Vec<u8>>>);
    impl Write for Tee {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn plan() -> OverridePlan {
        OverridePlan {
            knob: 2,
            from: 0.0,
            to: 1.0,
            duration: Duration::from_millis(250),
        }
    }

    fn acker(w: &Arc<Mutex<Vec<u8>>>, fail_after: Option<usize>) -> Acker {
        Acker {
            written: w.clone(),
            sent: 0,
            fail_after,
            seq: 0,
            pending: Vec::new(),
        }
    }

    #[test]
    fn sweep_sets_monotonic_then_releases() {
        let w = Arc::new(Mutex::new(Vec::new()));
        let d = tempfile::tempdir().unwrap();
        let mut out = Vec::new();
        let o = run_override(
            acker(&w, None),
            Tee(w.clone()),
            &plan(),
            &Policy::default(),
            &d.path().join("cap.txt"),
            &mut out,
        )
        .unwrap();
        let cmds = written_commands(&w);
        let values: Vec<u16> = cmds
            .iter()
            .filter_map(|c| match c {
                Command::OverrideSet { value, .. } => Some(*value),
                _ => None,
            })
            .collect();
        assert!(values.len() >= 2);
        assert_eq!(values[0], 0);
        assert_eq!(*values.last().unwrap(), 0xffff);
        assert!(values.windows(2).all(|p| p[0] < p[1]));
        assert_eq!(*cmds.last().unwrap(), Command::OverrideRelease { knob: 2 });
        assert_eq!(o, Outcome::Pass, "{}", String::from_utf8_lossy(&out));
        assert!(d.path().join("cap.txt").exists());
    }

    #[test]
    fn release_is_sent_when_capture_dies_mid_sweep() {
        let w = Arc::new(Mutex::new(Vec::new()));
        let d = tempfile::tempdir().unwrap();
        let r = run_override(
            acker(&w, Some(3)),
            Tee(w.clone()),
            &plan(),
            &Policy::default(),
            &d.path().join("cap.txt"),
            &mut Vec::new(),
        );
        assert!(matches!(r, Err(ControlError::Io(_))));
        assert_eq!(
            *written_commands(&w).last().unwrap(),
            Command::OverrideRelease { knob: 2 }
        );
    }

    #[test]
    fn rejected_set_fails_the_run() {
        let w = Arc::new(Mutex::new(Vec::new()));
        let mut a = frame_body(0, b"hi");
        a.extend(status_frame(1));
        a.extend(reply_frame(2, Reply::Error(ErrorCode::BadKnob)));
        let d = tempfile::tempdir().unwrap();
        let o = run_override(
            &a[..],
            Tee(w.clone()),
            &OverridePlan {
                from: 0.5,
                to: 0.5,
                ..plan()
            },
            &Policy::default(),
            &d.path().join("cap.txt"),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(o, Outcome::Fail);
        assert_eq!(
            *written_commands(&w).last().unwrap(),
            Command::OverrideRelease { knob: 2 }
        );
    }

    #[test]
    fn restart_waits_for_reenumeration() {
        let mut a = reply_frame(0, Reply::Ok(Verb::Restart));
        a.extend(frame_body(1, b"noise"));
        let (old, w_old) = Fake::new(vec![a]);
        let mut boot = frame_body(0, b"BOOT proto=1 fw=x");
        boot.extend(reply_frame(1, ver("0.3.2")));
        let (new, _) = Fake::new(vec![boot, reply_frame(2, rsn("pin-reset"))]);
        let mut new = Some(new);

        let p = PathBuf::from("/dev/cu.usbmodem201");
        let mut lists: VecDeque<Vec<PathBuf>> = VecDeque::from([
            vec![PathBuf::from("/dev/cu.usbmodem101")],
            vec![],
            vec![],
            vec![p.clone()],
        ]);
        let mut sleeps = 0;
        let r = restart(
            Link::new(old, T),
            || Ok(lists.pop_front().unwrap_or_default()),
            |path| {
                assert_eq!(path, p);
                Ok(new.take().expect("opened once"))
            },
            |_| sleeps += 1,
            RestartOpts {
                reenumerate_timeout: Duration::from_secs(15),
                poll: Duration::from_millis(100),
            },
        )
        .unwrap();
        assert_eq!(r.path, p);
        assert_eq!(r.identity.to_string(), "fw=0.3.2 reset_reason=pin-reset");
        assert!(r.boot_record);
        assert!(sleeps >= 2);
        assert_eq!(written_commands(&w_old), [Command::Restart]);
    }

    #[test]
    fn restart_times_out_when_console_never_returns() {
        let (old, _) = Fake::new(vec![reply_frame(0, Reply::Ok(Verb::Restart))]);
        let r = restart(
            Link::new(old, T),
            || Ok(Vec::new()),
            |_| -> io::Result<Fake> { unreachable!() },
            |_| {},
            RestartOpts {
                reenumerate_timeout: Duration::from_secs(1),
                poll: Duration::from_millis(100),
            },
        );
        assert!(matches!(r, Err(ControlError::NoDevice(_))));
    }

    #[test]
    fn restart_refused_by_device_is_not_followed() {
        let (old, _) = Fake::new(vec![reply_frame(0, Reply::Error(ErrorCode::Malformed))]);
        let r = restart(
            Link::new(old, T),
            || panic!("must not poll after a refusal"),
            |_| -> io::Result<Fake> { unreachable!() },
            |_| {},
            RestartOpts {
                reenumerate_timeout: Duration::from_secs(1),
                poll: Duration::from_millis(100),
            },
        );
        assert!(matches!(r, Err(ControlError::Rejected(_))));
    }
}
