//! One bench session: attach to a freshly booted rig, select the effect, wait for the capture to
//! arm, drive a knob on a timed schedule, release it, and keep every byte the port produced until
//! the audio dump ends. The raw stream is the artifact; [`write_session`] stores it zstd-compressed
//! beside a [`Sidecar`] naming what the host did.
//!
//! Everything here is generic over `Read + Write` so tests drive it with an in-memory device.
//! Opening the port, resetting over the probe and re-attaching afterwards are the binary's job
//! (see [`probe_reset`]).
//!
//! Two guarantees callers rely on:
//!
//! * `FX` is only ever sent after a `BOOT` record has been read on this attachment. A port that
//!   shows no `BOOT` is a board that has been running (and may already be armed), where an effect
//!   change would be refused or would make the recording unrepeatable, so the session stops
//!   without sending anything;
//! * once anything has been sent, the knob is released on every way out: a finished schedule, a
//!   write error, a read error, a timeout before the capture armed.

use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use asperitas_host::session::{sidecar_path, Mode, Sidecar};
use asperitas_wire::console::is_boot;
use asperitas_wire::control::{parse_reply, Command, ErrorCode, Reply, Verb};
use asperitas_wire::frame::{Decoder, Record, MAX_FRAME};
use ruzstd::encoding::{compress_to_vec, CompressionLevel};

use crate::cli::Outcome;
use crate::control::{frame_command, level_to_wire, schedule, ControlError};

/// Soak ramp: triangle from `SOAK_FROM` up to `SOAK_TO` and back over `SOAK_DURATION_S`, starting
/// `SOAK_START_S` after the capture arms. It starts and ends at the base value because a first
/// `SET` away from the base (or a `REL`) snaps the parameter, and a snap trips the discontinuity
/// detector whatever the smoothing.
const SOAK_FROM: f64 = 0.50;
const SOAK_TO: f64 = 0.95;
const SOAK_DURATION_S: f64 = 250.0;
const SOAK_START_S: f64 = 10.0;
const SOAK_HZ: f64 = 20.0;
/// How long a snap lasts.
const SNAP_WIDTH_S: f64 = 0.5;
/// Snap start times in the default soak, seconds from the start of the ramp.
pub const DEFAULT_SNAPS: [f64; 2] = [80.0, 160.0];
/// Level added during a default snap.
pub const DEFAULT_SNAP_LEVEL: f64 = 0.25;
/// The release follows the last `SET` of a sweep or soak by this much.
const RELEASE_LAG: Duration = Duration::from_millis(100);

/// How the knob is driven.
#[derive(Clone, Debug, PartialEq)]
pub enum Motion {
    /// Linear sweep at 20 `SET`s per second, starting `start_after_arm` after the capture arms.
    Sweep {
        from: f64,
        to: f64,
        duration: Duration,
        start_after_arm: Duration,
    },
    /// The long triangle ramp, with `snaps` (start times in seconds from the start of the ramp)
    /// raising the level by `snap_level` for half a second each.
    Soak { snaps: Vec<f64>, snap_level: f64 },
    /// One `SET` before the capture arms, held until the dump ends.
    Hold { level: f64 },
}

/// What a session asks the device to do.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionPlan {
    /// Effect selection (`00` bypass, `01` patch).
    pub fx_f: u8,
    /// Value sent with the effect selection.
    pub fx_v: u16,
    /// Knob id driven.
    pub knob: u8,
    pub motion: Motion,
}

/// The `SET`s and the release a [`SessionPlan`] expands to.
#[derive(Debug, PartialEq)]
pub struct Timeline {
    /// Wire values sent right after `FX`, before the capture arms.
    pub pre: Vec<u16>,
    /// `(offset after the capture armed, wire value)`, ascending.
    pub timed: Vec<(Duration, u16)>,
    /// Offset after arming at which to release; `None` releases when the dump ends.
    pub release_at: Option<Duration>,
}

impl SessionPlan {
    /// Expand the plan. A level outside `0..=1` is refused here, before any port is touched.
    pub fn timeline(&self) -> Result<Timeline, ControlError> {
        match &self.motion {
            Motion::Sweep {
                from,
                to,
                duration,
                start_after_arm,
            } => {
                let timed: Vec<_> = schedule(*from, *to, *duration)?
                    .into_iter()
                    .map(|(t, v)| (t + *start_after_arm, v))
                    .collect();
                Ok(Timeline {
                    pre: Vec::new(),
                    timed,
                    release_at: Some(*start_after_arm + *duration + RELEASE_LAG),
                })
            }
            Motion::Soak { snaps, snap_level } => {
                let steps = (SOAK_DURATION_S * SOAK_HZ) as usize;
                let start = Duration::from_secs_f64(SOAK_START_S);
                let mut timed = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let t = i as f64 / SOAK_HZ;
                    let x = t / SOAK_DURATION_S;
                    let tri = if x < 0.5 { 2.0 * x } else { 2.0 * (1.0 - x) };
                    let mut v = SOAK_FROM + (SOAK_TO - SOAK_FROM) * tri;
                    if snaps.iter().any(|&s| s <= t && t < s + SNAP_WIDTH_S) {
                        v = (v + snap_level).min(1.0);
                    }
                    timed.push((start + Duration::from_secs_f64(t), level_to_wire(v)?));
                }
                Ok(Timeline {
                    pre: Vec::new(),
                    timed,
                    release_at: Some(
                        start + Duration::from_secs_f64(SOAK_DURATION_S) + RELEASE_LAG,
                    ),
                })
            }
            Motion::Hold { level } => Ok(Timeline {
                pre: vec![level_to_wire(*level)?],
                timed: Vec::new(),
                release_at: None,
            }),
        }
    }

    /// The sidecar for this plan, with the device times the recorder read from the stream.
    pub fn sidecar(&self, arm_t_ms: Option<u32>, first_set_t_ms: Option<u32>) -> Sidecar {
        let (mode, from, to, duration_s, start_after_arm_s, snaps, snap_level) = match &self.motion
        {
            Motion::Sweep {
                from,
                to,
                duration,
                start_after_arm,
            } => (
                Mode::Sweep,
                *from,
                *to,
                duration.as_secs_f64(),
                start_after_arm.as_secs_f64(),
                Vec::new(),
                0.0,
            ),
            Motion::Soak { snaps, snap_level } => (
                Mode::Soak,
                SOAK_FROM,
                SOAK_TO,
                SOAK_DURATION_S,
                SOAK_START_S,
                snaps.clone(),
                *snap_level,
            ),
            Motion::Hold { level } => (Mode::Hold, *level, *level, 0.0, 0.0, Vec::new(), 0.0),
        };
        Sidecar {
            fx_f: self.fx_f,
            fx_v: self.fx_v,
            knob: self.knob,
            mode,
            from,
            to,
            duration_s,
            start_after_arm_s,
            snaps,
            snap_level,
            arm_t_ms,
            first_set_t_ms,
        }
    }
}

/// Time limits of a session.
#[derive(Clone, Copy, Debug)]
pub struct SessionOpts {
    /// Whole-session limit, from attach to the end of the dump.
    pub max: Duration,
    /// How long to wait for a `BOOT` record before refusing to send anything.
    pub boot_wait: Duration,
    /// How long to wait for the `FX` acknowledgement.
    pub reply_timeout: Duration,
    /// How long to keep reading after `DUMPEND`.
    pub dump_grace: Duration,
}

impl SessionOpts {
    /// The limits the bench uses; only the session limit is a choice.
    pub fn bench(max: Duration) -> Self {
        Self {
            max,
            boot_wait: Duration::from_secs(12),
            reply_timeout: Duration::from_secs(2),
            dump_grace: Duration::from_secs(2),
        }
    }
}

/// What the stream showed while the session ran.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Observed {
    /// `BOOT` records read. More than one after the session sent a command means a device reset.
    pub boots: usize,
    /// `CTLR OK FX` was read.
    pub fx_acked: bool,
    /// `CTLR OK SET` replies.
    pub set_acks: usize,
    /// `CTLR OK REL` replies.
    pub rel_acks: usize,
    /// `CTLR ERR` replies.
    pub errors: usize,
    /// The reason of the latest `CTLR ERR`.
    pub last_error: Option<ErrorCode>,
    /// A `CAPSTAT` with `delivered > 0` was read.
    pub armed: bool,
    /// Device `t_ms` of that record.
    pub arm_t_ms: Option<u32>,
    /// Device `t_ms` of the first `CTLR OK SET`.
    pub first_set_t_ms: Option<u32>,
    /// `DUMPEND` was read.
    pub dump_end: bool,
}

impl Observed {
    fn observe(&mut self, r: &Record<'_>) {
        if is_boot(r.body) {
            self.boots += 1;
        } else if let Some(delivered) = capstat_delivered(r.body) {
            if delivered > 0 && !self.armed {
                self.armed = true;
                self.arm_t_ms = Some(r.t_ms);
            }
        } else if r.body.starts_with(b"DUMPEND ") {
            self.dump_end = true;
        } else {
            match parse_reply(r.body) {
                Ok(Reply::Ok(Verb::Fx)) => self.fx_acked = true,
                Ok(Reply::Ok(Verb::Set)) => {
                    self.set_acks += 1;
                    self.first_set_t_ms.get_or_insert(r.t_ms);
                }
                Ok(Reply::Ok(Verb::Release)) => self.rel_acks += 1,
                Ok(Reply::Error(code)) => {
                    self.errors += 1;
                    self.last_error = Some(code);
                }
                _ => {}
            }
        }
    }
}

/// `delivered=N` of a `CAPSTAT` body, or `None` for any other record.
fn capstat_delivered(body: &[u8]) -> Option<u32> {
    let rest = body.strip_prefix(b"CAPSTAT ")?;
    let field = rest
        .split(|&b| b == b' ')
        .find_map(|tok| tok.strip_prefix(b"delivered="))?;
    std::str::from_utf8(field).ok()?.parse().ok()
}

/// A finished (or aborted) session. `raw` holds every byte read, even when `error` is set, so a
/// partial run is still evidence.
#[derive(Debug)]
pub struct SessionRun {
    pub raw: Vec<u8>,
    pub observed: Observed,
    /// `SET` commands sent, including the pre-arm ones.
    pub sets_sent: usize,
    /// Why the session stopped early, if it did.
    pub error: Option<ControlError>,
}

impl SessionRun {
    /// Pass only when the session ran to `DUMPEND` and the device acknowledged the `FX`, every
    /// `SET` and the `REL`, and rejected nothing.
    pub fn outcome(&self) -> Outcome {
        let o = &self.observed;
        let clean = self.error.is_none()
            && o.fx_acked
            && o.dump_end
            && o.set_acks == self.sets_sent
            && o.rel_acks >= 1
            && o.errors == 0;
        if clean {
            Outcome::Pass
        } else if self
            .error
            .as_ref()
            .is_some_and(|e| e.outcome() == Outcome::Error)
        {
            Outcome::Error
        } else {
            Outcome::Fail
        }
    }

    /// One summary line.
    pub fn summary(&self) -> String {
        let o = &self.observed;
        format!(
            "session bytes={} fx_acked={} sets_sent={} set_acks={} rel_acks={} device_errors={} \
             armed={} dump_end={}",
            self.raw.len(),
            o.fx_acked,
            self.sets_sent,
            o.set_acks,
            o.rel_acks,
            o.errors,
            o.armed,
            o.dump_end,
        )
    }
}

struct Runner<P> {
    port: P,
    decoder: Decoder,
    raw: Vec<u8>,
    observed: Observed,
    seq: u32,
    sent_anything: bool,
    released: bool,
    sets_sent: usize,
}

impl<P: Read + Write> Runner<P> {
    fn send(&mut self, cmd: Command) -> Result<(), ControlError> {
        let mut frame = [0u8; MAX_FRAME];
        let n = frame_command(cmd, self.seq, &mut frame);
        self.seq = self.seq.wrapping_add(1);
        self.sent_anything = true;
        self.port.write_all(&frame[..n])?;
        self.port.flush()?;
        Ok(())
    }

    fn send_set(&mut self, knob: u8, value: u16) -> Result<(), ControlError> {
        self.sets_sent += 1;
        self.send(Command::OverrideSet { knob, value })
    }

    fn release(&mut self, knob: u8) -> Result<(), ControlError> {
        self.released = true;
        self.send(Command::OverrideRelease { knob })
    }

    /// One read: whatever arrives is kept verbatim and offered to the decoder. A port timeout is
    /// not an error; it is how the loop gets control back to run the schedule.
    fn pump(&mut self) -> Result<(), ControlError> {
        let mut chunk = [0u8; 4096];
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
                return Ok(())
            }
            Err(e) => return Err(e.into()),
        };
        self.raw.extend_from_slice(&chunk[..n]);
        let observed = &mut self.observed;
        let taken = self.decoder.feed(&chunk[..n], |r| observed.observe(r));
        if taken < n {
            return Err(ControlError::Io(io::Error::other(
                "decoder made no progress",
            )));
        }
        if self.sent_anything && self.observed.boots > 1 {
            return Err(ControlError::DeviceReset);
        }
        Ok(())
    }

    fn drive(
        &mut self,
        plan: &SessionPlan,
        timeline: &Timeline,
        opts: &SessionOpts,
    ) -> Result<(), ControlError> {
        let start = Instant::now();
        let limit = start + opts.max;

        let boot_deadline = start + opts.boot_wait.min(opts.max);
        while self.observed.boots == 0 {
            if Instant::now() >= boot_deadline {
                return Err(ControlError::NoDevice(
                    "no BOOT record after attaching: this is not a fresh boot, refusing to send FX"
                        .into(),
                ));
            }
            self.pump()?;
        }

        self.send(Command::ReplayFx {
            fx: plan.fx_f,
            value: plan.fx_v,
        })?;
        let errors_before = self.observed.errors;
        let fx_deadline = Instant::now() + opts.reply_timeout;
        while !self.observed.fx_acked {
            if self.observed.errors > errors_before {
                return Err(ControlError::Rejected(
                    self.observed.last_error.unwrap_or(ErrorCode::Malformed),
                ));
            }
            if Instant::now() >= fx_deadline.min(limit) {
                return Err(ControlError::Timeout);
            }
            self.pump()?;
        }
        for &value in &timeline.pre {
            self.send_set(plan.knob, value)?;
        }

        while !self.observed.armed {
            if Instant::now() >= limit {
                return Err(ControlError::Timeout);
            }
            self.pump()?;
        }
        let armed_at = Instant::now();

        let mut next = 0;
        let mut dump_seen_at: Option<Instant> = None;
        loop {
            let elapsed = armed_at.elapsed();
            while next < timeline.timed.len() && timeline.timed[next].0 <= elapsed {
                self.send_set(plan.knob, timeline.timed[next].1)?;
                next += 1;
            }
            let due = timeline.release_at.is_some_and(|r| elapsed >= r);
            if !self.released && next == timeline.timed.len() && due {
                self.release(plan.knob)?;
            }
            if dump_seen_at.is_none() && self.observed.dump_end {
                dump_seen_at = Some(Instant::now());
                // A hold has no scheduled release; the dump ending is the cue. The grace below
                // keeps reading so the acknowledgement lands in the stream.
                if !self.released {
                    self.release(plan.knob)?;
                }
            }
            if dump_seen_at.is_some_and(|t| t.elapsed() >= opts.dump_grace) {
                return Ok(());
            }
            if Instant::now() >= limit {
                return Err(ControlError::Timeout);
            }
            self.pump()?;
        }
    }
}

/// Run `plan` against `port` and return everything it produced. The port should have a short read
/// timeout (the binary uses 10 ms): the schedule runs between reads, so the timeout is its jitter.
pub fn run_session<P: Read + Write>(port: P, plan: &SessionPlan, opts: &SessionOpts) -> SessionRun {
    let timeline = match plan.timeline() {
        Ok(t) => t,
        Err(e) => {
            return SessionRun {
                raw: Vec::new(),
                observed: Observed::default(),
                sets_sent: 0,
                error: Some(e),
            }
        }
    };
    let mut r = Runner {
        port,
        decoder: Decoder::new(),
        raw: Vec::new(),
        observed: Observed::default(),
        seq: 0,
        sent_anything: false,
        released: false,
        sets_sent: 0,
    };
    let mut error = r.drive(plan, &timeline, opts).err();
    if r.sent_anything && !r.released {
        // Best effort: the failure that got us here is the one worth reporting.
        if let Err(e) = r.release(plan.knob) {
            error.get_or_insert(e);
        }
    }
    SessionRun {
        raw: r.raw,
        observed: r.observed,
        sets_sent: r.sets_sent,
        error,
    }
}

/// Write the raw stream zstd-compressed to `out` and the sidecar beside it. Returns `Ok(false)`
/// without writing anything when the session read nothing.
pub fn write_session(out: &Path, plan: &SessionPlan, run: &SessionRun) -> io::Result<bool> {
    if run.raw.is_empty() {
        return Ok(false);
    }
    std::fs::write(
        out,
        compress_to_vec(run.raw.as_slice(), CompressionLevel::Fastest),
    )?;
    let sidecar = plan.sidecar(run.observed.arm_t_ms, run.observed.first_set_t_ms);
    std::fs::write(sidecar_path(out), sidecar.render())?;
    Ok(true)
}

/// Pick the error a finished session reports. The session's own failure (timeout, closed port,
/// rejected command) is the root cause, so it wins over an artifact write failure; the write
/// failure is returned only when the session itself succeeded.
pub fn session_error(
    run_error: Option<ControlError>,
    write: io::Result<bool>,
) -> Option<ControlError> {
    run_error.or(write.err().map(ControlError::Io))
}

/// Reset the board over the probe. The console vanishes and re-enumerates afterwards; the caller
/// waits for that and re-attaches.
pub fn probe_reset(chip_description: &Path) -> Result<(), ControlError> {
    let out = std::process::Command::new("probe-rs")
        .args([
            "reset",
            "--chip",
            "ASPERITAS_H750IB",
            "--chip-description-path",
        ])
        .arg(chip_description)
        .args(["--non-interactive", "--connect-under-reset"])
        .output()
        .map_err(|e| ControlError::NoDevice(format!("cannot run probe-rs: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ControlError::NoDevice(format!(
            "probe-rs reset failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_wire::control::{parse_command, Reply};
    use asperitas_wire::frame::{encode, MAX_BODY};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn frame_body(seq: u32, t_ms: u32, body: &[u8]) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let e = encode(asperitas_wire::Level::Info, seq, t_ms, body, &mut buf);
        buf[..e.len].to_vec()
    }

    fn reply_frame(seq: u32, r: Reply) -> Vec<u8> {
        let mut b = [0u8; MAX_BODY];
        let n = r.encode(&mut b);
        frame_body(seq, seq * 10, &b[..n])
    }

    /// An in-memory rig. It speaks first with `BOOT` (unless `boot` is off), acknowledges every
    /// command it is sent, reports the capture armed once `FX` is acknowledged (unless `arm` is
    /// off) and ends the dump when it sees a `REL`.
    struct Rig {
        boot: bool,
        arm: bool,
        die_after_arm: bool,
        dump_on_arm: bool,
        /// After the nth write (0-based) is acknowledged, the device restarts and sends `BOOT`.
        reset_on_write: Option<usize>,
        /// The nth write (0-based) fails once.
        fail_write: Option<usize>,
        writes: usize,
        queue: VecDeque<Vec<u8>>,
        seq: u32,
        written: Arc<Mutex<Vec<Command>>>,
        armed: bool,
    }

    impl Rig {
        fn new() -> (Self, Arc<Mutex<Vec<Command>>>) {
            let written = Arc::new(Mutex::new(Vec::new()));
            let mut rig = Self {
                boot: true,
                arm: true,
                die_after_arm: false,
                dump_on_arm: false,
                fail_write: None,
                reset_on_write: None,
                writes: 0,
                queue: VecDeque::new(),
                seq: 0,
                written: written.clone(),
                armed: false,
            };
            let boot = rig.next_frame(b"BOOT proto=1 fw=x");
            rig.queue.push_back(boot);
            (rig, written)
        }
        fn next_frame(&mut self, body: &[u8]) -> Vec<u8> {
            self.seq += 1;
            frame_body(self.seq, 1000 + self.seq * 10, body)
        }
    }

    impl Read for Rig {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.armed && self.die_after_arm {
                return Err(io::Error::other("unplugged"));
            }
            let next = if self.boot {
                self.queue.pop_front()
            } else {
                None
            };
            match next {
                Some(v) => {
                    buf[..v.len()].copy_from_slice(&v);
                    Ok(v.len())
                }
                None => {
                    std::thread::sleep(Duration::from_millis(1));
                    Err(io::ErrorKind::TimedOut.into())
                }
            }
        }
    }

    impl Write for Rig {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            let n = self.writes;
            self.writes += 1;
            if self.fail_write == Some(n) {
                return Err(io::Error::other("write failed"));
            }
            if self.reset_on_write == Some(n) {
                let f = self.next_frame(b"BOOT proto=1 fw=x");
                self.queue.push_back(f);
            }
            let mut d = Decoder::new();
            d.push(b);
            while let Some(r) = d.next_record() {
                let cmd = parse_command(r.body).expect("valid command");
                self.written.lock().unwrap().push(cmd);
                let reply = reply_frame(self.seq, Reply::Ok(cmd.verb()));
                self.seq += 1;
                self.queue.push_back(reply);
                match cmd {
                    Command::ReplayFx { .. } if self.arm => {
                        let f = self.next_frame(
                            b"CAPSTAT proto=1 delivered=3 expected=1 overrun=0 max_block_us=1 worst_gap_us=1 audio_exit=0 dumped=0 dropped_full=0",
                        );
                        self.queue.push_back(f);
                        self.armed = true;
                        if self.dump_on_arm {
                            let f = self.next_frame(DUMPEND);
                            self.queue.push_back(f);
                        }
                    }
                    Command::OverrideRelease { .. } => {
                        let f = self.next_frame(DUMPEND);
                        self.queue.push_back(f);
                    }
                    _ => {}
                }
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    const DUMPEND: &[u8] = b"DUMPEND proto=1 blocks=1 chunks=1 bytes=1 elapsed_ms=1 refused=0 stall_ms=0 sent=1 dropped_full=0 bytes_dropped=0";

    fn opts(max_ms: u64) -> SessionOpts {
        SessionOpts {
            max: Duration::from_millis(max_ms),
            boot_wait: Duration::from_millis(100),
            reply_timeout: Duration::from_millis(100),
            dump_grace: Duration::from_millis(20),
        }
    }

    fn sweep_plan() -> SessionPlan {
        SessionPlan {
            fx_f: 1,
            fx_v: 0,
            knob: 2,
            motion: Motion::Sweep {
                from: 0.0,
                to: 1.0,
                duration: Duration::from_millis(200),
                start_after_arm: Duration::from_millis(20),
            },
        }
    }

    fn cmds(w: &Arc<Mutex<Vec<Command>>>) -> Vec<Command> {
        w.lock().unwrap().clone()
    }

    #[test]
    fn sweep_session_runs_in_order_and_releases_last() {
        let (rig, written) = Rig::new();
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(run.error.is_none(), "{:?}", run.error);
        let c = cmds(&written);
        assert_eq!(c[0], Command::ReplayFx { fx: 1, value: 0 });
        let sets: Vec<u16> = c
            .iter()
            .filter_map(|c| match c {
                Command::OverrideSet { knob: 2, value } => Some(*value),
                _ => None,
            })
            .collect();
        assert!(sets.len() >= 2);
        assert_eq!(sets[0], 0);
        assert_eq!(*sets.last().unwrap(), 0xffff);
        assert_eq!(c.last(), Some(&Command::OverrideRelease { knob: 2 }));
        assert_eq!(
            c.iter()
                .filter(|c| matches!(c, Command::OverrideRelease { .. }))
                .count(),
            1
        );
        assert_eq!(run.sets_sent, sets.len());
        assert_eq!(run.observed.set_acks, sets.len());
        assert!(run.observed.dump_end && run.observed.armed);
        assert!(run.observed.arm_t_ms.is_some() && run.observed.first_set_t_ms.is_some());
        assert_eq!(run.outcome(), Outcome::Pass, "{}", run.summary());
    }

    #[test]
    fn raw_stream_and_sidecar_are_written() {
        let (rig, _) = Rig::new();
        let plan = sweep_plan();
        let run = run_session(rig, &plan, &opts(3000));
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("s.console.zst");
        assert!(write_session(&out, &plan, &run).unwrap());
        let mut back = Vec::new();
        ruzstd::decoding::StreamingDecoder::new(std::fs::File::open(&out).unwrap())
            .unwrap()
            .read_to_end(&mut back)
            .unwrap();
        assert_eq!(back, run.raw);
        let side = Sidecar::parse(&std::fs::read_to_string(sidecar_path(&out)).unwrap()).unwrap();
        assert_eq!(
            side,
            plan.sidecar(run.observed.arm_t_ms, run.observed.first_set_t_ms)
        );
    }

    #[test]
    fn empty_raw_stream_writes_no_artifact() {
        let plan = sweep_plan();
        let run = SessionRun {
            raw: Vec::new(),
            observed: Observed::default(),
            sets_sent: 0,
            error: Some(ControlError::Timeout),
        };
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("s.console.zst");
        assert!(!write_session(&out, &plan, &run).unwrap());
        assert!(!out.exists() && !sidecar_path(&out).exists());
    }

    #[test]
    fn session_failure_wins_over_write_failure() {
        let werr = || Err(io::Error::other("disk"));
        assert!(matches!(
            session_error(Some(ControlError::Timeout), werr()),
            Some(ControlError::Timeout)
        ));
        assert!(matches!(
            session_error(None, werr()),
            Some(ControlError::Io(_))
        ));
        assert!(session_error(None, Ok(false)).is_none());
    }

    #[test]
    fn hold_sets_before_arm_and_releases_at_dump_end() {
        let (mut rig, written) = Rig::new();
        rig.dump_on_arm = true;
        let plan = SessionPlan {
            motion: Motion::Hold { level: 0.5 },
            ..sweep_plan()
        };
        let run = run_session(rig, &plan, &opts(3000));
        assert_eq!(run.outcome(), Outcome::Pass, "{}", run.summary());
        assert_eq!(
            cmds(&written),
            [
                Command::ReplayFx { fx: 1, value: 0 },
                Command::OverrideSet {
                    knob: 2,
                    value: 0x8000
                },
                Command::OverrideRelease { knob: 2 },
            ]
        );
    }

    #[test]
    fn release_is_sent_when_a_set_write_fails() {
        let (mut rig, written) = Rig::new();
        rig.fail_write = Some(2); // FX, first SET, then this one
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::Io(_))));
        assert_eq!(
            cmds(&written).last(),
            Some(&Command::OverrideRelease { knob: 2 })
        );
        assert_eq!(run.outcome(), Outcome::Error);
    }

    #[test]
    fn release_is_sent_when_the_read_side_dies() {
        let (mut rig, written) = Rig::new();
        rig.die_after_arm = true;
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::Io(_))));
        assert_eq!(
            cmds(&written).last(),
            Some(&Command::OverrideRelease { knob: 2 })
        );
    }

    #[test]
    fn release_is_sent_when_the_capture_never_arms() {
        let (mut rig, written) = Rig::new();
        rig.arm = false;
        let run = run_session(rig, &sweep_plan(), &opts(150));
        assert!(matches!(run.error, Some(ControlError::Timeout)));
        let c = cmds(&written);
        assert_eq!(c.last(), Some(&Command::OverrideRelease { knob: 2 }));
        assert!(!c.iter().any(|c| matches!(c, Command::OverrideSet { .. })));
    }

    #[test]
    fn mid_session_boot_aborts_with_device_reset() {
        let (mut rig, written) = Rig::new();
        rig.reset_on_write = Some(2);
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(
            matches!(run.error, Some(ControlError::DeviceReset)),
            "{:?}",
            run.error
        );
        assert_eq!(run.outcome(), Outcome::Error);
        let sets = cmds(&written)
            .iter()
            .filter(|c| matches!(c, Command::OverrideSet { .. }))
            .count();
        assert!(sets <= 2, "kept sending SETs after the reset: {sets}");
    }

    #[test]
    fn no_boot_sends_nothing() {
        let (mut rig, written) = Rig::new();
        rig.boot = false;
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::NoDevice(_))));
        assert!(cmds(&written).is_empty());
    }

    #[test]
    fn out_of_range_plan_touches_nothing() {
        let (rig, written) = Rig::new();
        let plan = SessionPlan {
            motion: Motion::Sweep {
                from: 0.0,
                to: 1.5,
                duration: Duration::from_secs(1),
                start_after_arm: Duration::ZERO,
            },
            ..sweep_plan()
        };
        let run = run_session(rig, &plan, &opts(3000));
        assert!(matches!(run.error, Some(ControlError::Usage(_))));
        assert!(cmds(&written).is_empty());
        assert!(run.raw.is_empty());
    }

    #[test]
    fn rejected_fx_stops_the_session() {
        struct Refuser(Rig);
        impl Read for Refuser {
            fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                self.0.read(b)
            }
        }
        impl Write for Refuser {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                let n = b.len();
                self.0
                    .queue
                    .push_back(reply_frame(50, Reply::Error(ErrorCode::BadValue)));
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (rig, _) = Rig::new();
        let run = run_session(Refuser(rig), &sweep_plan(), &opts(3000));
        assert!(matches!(
            run.error,
            Some(ControlError::Rejected(ErrorCode::BadValue))
        ));
        assert_eq!(run.outcome(), Outcome::Fail);
    }

    #[test]
    fn sweep_timeline_is_offset_by_start_after_arm() {
        let t = sweep_plan().timeline().unwrap();
        assert_eq!(t.timed[0], (Duration::from_millis(20), 0));
        assert_eq!(t.timed.last().unwrap().1, 0xffff);
        assert_eq!(t.release_at, Some(Duration::from_millis(320)));
        assert!(t.pre.is_empty());
    }

    #[test]
    fn soak_timeline_is_a_triangle_with_snaps() {
        let plan = SessionPlan {
            motion: Motion::Soak {
                snaps: DEFAULT_SNAPS.to_vec(),
                snap_level: DEFAULT_SNAP_LEVEL,
            },
            ..sweep_plan()
        };
        let t = plan.timeline().unwrap();
        assert_eq!(t.timed.len(), 5001);
        assert_eq!(t.timed[0], (Duration::from_secs(10), 0x8000));
        assert_eq!(t.timed.last().unwrap().1, 0x8000);
        let peak = t.timed[..1600]
            .iter()
            .chain(&t.timed[1700..3200])
            .map(|&(_, v)| v)
            .max();
        assert_eq!(peak, Some(level_to_wire(0.95).unwrap()));
        // Ten samples of 50 ms inside each half-second snap sit 0.25 above the ramp.
        let at = |s: f64| t.timed[(s * 20.0) as usize].1;
        assert!(at(80.0) > level_to_wire(0.50 + 0.45 * 0.64).unwrap());
        assert!(at(80.5) < at(80.0));
        let none = SessionPlan {
            motion: Motion::Soak {
                snaps: vec![],
                snap_level: 0.25,
            },
            ..sweep_plan()
        };
        assert!(none
            .timeline()
            .unwrap()
            .timed
            .iter()
            .all(|&(_, v)| v <= level_to_wire(0.95).unwrap()));
        assert_eq!(t.release_at, Some(Duration::from_millis(260_100)));
    }

    #[test]
    fn hold_timeline_is_one_pre_arm_set() {
        let plan = SessionPlan {
            motion: Motion::Hold { level: 1.0 },
            ..sweep_plan()
        };
        assert_eq!(
            plan.timeline().unwrap(),
            Timeline {
                pre: vec![0xffff],
                timed: vec![],
                release_at: None
            }
        );
        let bad = SessionPlan {
            motion: Motion::Hold { level: -0.1 },
            ..sweep_plan()
        };
        assert!(bad.timeline().is_err());
    }

    #[test]
    fn capstat_delivered_parses_only_capstat() {
        assert_eq!(
            capstat_delivered(b"CAPSTAT proto=1 delivered=7 expected=1"),
            Some(7)
        );
        assert_eq!(capstat_delivered(b"STATUS proto=1 delivered=7"), None);
        assert_eq!(capstat_delivered(b"CAPSTAT proto=1 expected=1"), None);
    }
}
