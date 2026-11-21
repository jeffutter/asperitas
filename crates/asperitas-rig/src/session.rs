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

use asperitas_dsp::asperitas::Controls;
use asperitas_dsp::automation::{Automation, ControlSurface};
use asperitas_dsp::resonator_controls::ResonatorControls;
use asperitas_host::session::{sidecar_path, Mode, Sidecar};
use asperitas_wire::console::is_boot;
use asperitas_wire::control::{parse_reply, Command, ErrorCode, Reply, Verb};
use asperitas_wire::frame::{Decoder, Record, MAX_FRAME};
use ruzstd::encoding::{compress_to_vec, CompressionLevel};

use crate::cli::Outcome;
use crate::control::{frame_command, level_to_wire, schedule, ControlError};
use crate::knob::FX_RESONATOR;

/// Sample rate the automation clock counts frames at: the device's.
pub const DEVICE_SAMPLE_RATE: u32 = 48_000;

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
    /// The `from` level is sent once before the capture arms, so the knob override is engaged and
    /// settled at the start of the sweep instead of the device still sitting at its default.
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
    /// Every knob follows an automation script (`docs/reference/automation.md`), its time zero at
    /// the capture arming. `text` is the file's contents, read once so the session and its
    /// sidecar carry the same script. Each knob is overridden from before the arming at the
    /// script's starting value (the default for a knob the script never names), so the board does
    /// not follow a physical knob, and released together after the last update.
    Automation { text: String },
}

/// What a session asks the device to do.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionPlan {
    /// Effect selection (`00` bypass, `01` patch, `02` resonator).
    pub fx_f: u8,
    /// Value sent with the effect selection.
    pub fx_v: u16,
    /// Knob id driven by a sweep, soak or hold; `None` for an automation, which names its knobs.
    pub knob: Option<u8>,
    pub motion: Motion,
}

/// The `SET`s and the release a [`SessionPlan`] expands to.
#[derive(Debug, PartialEq)]
pub struct Timeline {
    /// `(knob, wire value)` sent right after `FX`, before the capture arms.
    pub pre: Vec<(u8, u16)>,
    /// `(offset after the capture armed, knob, wire value)`, ascending by offset.
    pub timed: Vec<(Duration, u8, u16)>,
    /// Every knob the session overrides, released together.
    pub knobs: Vec<u8>,
    /// Offset after arming at which to release; `None` releases when the dump ends.
    pub release_at: Option<Duration>,
}

impl SessionPlan {
    /// Expand the plan. A level outside `0..=1` is refused here, before any port is touched.
    pub fn timeline(&self) -> Result<Timeline, ControlError> {
        if let Motion::Automation { text } = &self.motion {
            if self.knob.is_some() {
                return Err(ControlError::Usage(
                    "an automation names its own knobs; --knob does not apply".into(),
                ));
            }
            return if self.fx_f == FX_RESONATOR {
                automation_timeline::<ResonatorControls>(text)
            } else {
                automation_timeline::<Controls>(text)
            };
        }
        let knob = self.knob.ok_or_else(|| {
            ControlError::Usage("--knob is required for sweep, soak and hold".into())
        })?;
        match &self.motion {
            Motion::Sweep {
                from,
                to,
                duration,
                start_after_arm,
            } => {
                let timed: Vec<_> = schedule(*from, *to, *duration)?
                    .into_iter()
                    .map(|(t, v)| (t + *start_after_arm, knob, v))
                    .collect();
                Ok(Timeline {
                    pre: vec![(knob, level_to_wire(*from)?)],
                    timed,
                    knobs: vec![knob],
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
                    timed.push((start + Duration::from_secs_f64(t), knob, level_to_wire(v)?));
                }
                Ok(Timeline {
                    pre: Vec::new(),
                    timed,
                    knobs: vec![knob],
                    release_at: Some(
                        start + Duration::from_secs_f64(SOAK_DURATION_S) + RELEASE_LAG,
                    ),
                })
            }
            Motion::Hold { level } => Ok(Timeline {
                pre: vec![(knob, level_to_wire(*level)?)],
                timed: Vec::new(),
                knobs: vec![knob],
                release_at: None,
            }),
            Motion::Automation { .. } => unreachable!("expanded above"),
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
            Motion::Automation { text } => {
                // A script that does not parse never reaches a sidecar: `timeline` refuses it
                // before the session starts.
                let end = if self.fx_f == FX_RESONATOR {
                    last_update_frame::<ResonatorControls>(text)
                } else {
                    last_update_frame::<Controls>(text)
                };
                (
                    Mode::Automation,
                    0.0,
                    0.0,
                    end as f64 / f64::from(DEVICE_SAMPLE_RATE),
                    0.0,
                    Vec::new(),
                    0.0,
                )
            }
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
            automation: match &self.motion {
                Motion::Automation { text } => Some(text.clone()),
                _ => None,
            },
            arm_t_ms,
            first_set_t_ms,
        }
    }
}

/// The frame of the script's last device update; 0 for an empty or unparsable script.
fn last_update_frame<C: ControlSurface>(text: &str) -> u64 {
    Automation::<C>::parse_for(text, DEVICE_SAMPLE_RATE)
        .ok()
        .and_then(|a| a.device_schedule().last().map(|e| e.0))
        .unwrap_or(0)
}

/// Expand an automation script for control surface `C`. Every knob is overridden at its starting value before the capture
/// arms, then each update of [`Automation::device_schedule`] goes out at its frame's time. The
/// wire carries a level in `0..=1`, so each knob position goes through the same
/// `level_to_wire` the other motions use; a position outside it is refused, never clamped.
fn automation_timeline<C: ControlSurface>(text: &str) -> Result<Timeline, ControlError> {
    let automation = Automation::<C>::parse_for(text, DEVICE_SAMPLE_RATE)
        .map_err(|e| ControlError::Usage(format!("automation: {e}")))?;
    let at = |frame: u64| Duration::from_secs_f64(frame as f64 / f64::from(DEVICE_SAMPLE_RATE));
    let wire = |position: f32| level_to_wire(f64::from(position));
    let start = automation.controls_at(0);
    let knobs: Vec<u8> = (0..start.positions().len() as u8).collect();
    let pre = start
        .positions()
        .iter()
        .enumerate()
        .map(|(k, &p)| Ok((k as u8, wire(p)?)))
        .collect::<Result<Vec<_>, ControlError>>()?;
    let mut timed = Vec::new();
    for (frame, knob, position) in automation.device_schedule() {
        timed.push((at(frame), knob as u8, wire(position)?));
    }
    // The render holds each knob at its last value to the end of the clip, so the board does too:
    // the release waits for the end of the dump, as for a hold.
    Ok(Timeline {
        pre,
        timed,
        knobs,
        release_at: None,
    })
}

/// Time limits of a session.
#[derive(Clone, Copy, Debug)]
pub struct SessionOpts {
    /// Whole-session limit, from attach to the end of the dump.
    pub max: Duration,
    /// How long to wait for a fresh-boot proof (`BOOT`, or an early idle stream) before refusing to send anything.
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
    /// Sequence number and device `t_ms` of the first record read.
    pub first: Option<(u32, u32)>,
    /// A `CAPSTAT` with `delivered=0` was read: the capture had not started.
    pub idle_capstat: bool,
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
    /// The capture armed: the `rig: capture armed` log record was read, or, failing that, a
    /// `CAPSTAT` with `delivered > 0`.
    pub armed: bool,
    /// Device `t_ms` of that record. The log record is sent at the instant of arming, so this is
    /// exact to the millisecond; `CAPSTAT` comes about once a second and can be up to a second
    /// later, so it is only the fallback for a stream that missed the log record.
    pub arm_t_ms: Option<u32>,
    /// Device `t_ms` of the first `CTLR OK SET`.
    pub first_set_t_ms: Option<u32>,
    /// `DUMPEND` was read.
    pub dump_end: bool,
}

impl Observed {
    fn observe(&mut self, r: &Record<'_>) {
        self.first.get_or_insert((r.seq, r.t_ms));
        if is_boot(r.body) {
            self.boots += 1;
        } else if r.body.starts_with(CAPTURE_ARMED_LOG) {
            self.armed = true;
            self.arm_t_ms.get_or_insert(r.t_ms);
        } else if let Some(delivered) = capstat_delivered(r.body) {
            if delivered == 0 {
                self.idle_capstat = true;
            }
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

/// A board whose first record is this early (device `t_ms`) and this low (`seq`) was attached
/// within moments of starting. The USB console only connects a few hundred ms into a bootloaded
/// start, so the `BOOT` record is lost before the host can attach; this bound is how much of the
/// start may be missing and still count as fresh.
const FRESH_MAX_T_MS: u32 = 3000;
const FRESH_MAX_SEQ: u32 = 32;

impl Observed {
    /// The board is safe to send `FX` to: either its `BOOT` record was read, or the stream began
    /// early in the device's life and shows the capture idle (`CAPSTAT delivered=0`) and never armed.
    fn fresh_boot(&self) -> bool {
        if self.boots > 0 {
            return true;
        }
        matches!(self.first, Some((seq, t_ms)) if seq <= FRESH_MAX_SEQ && t_ms <= FRESH_MAX_T_MS)
            && self.idle_capstat
            && !self.armed
    }
}

/// Start of the log line the rig firmware writes at the instant the capture window opens
/// (`rig: capture armed for N s ...`, or `... until the ring is full ...`), from `run_capture` in
/// `firmware/src/bin/rig/timeline.rs`. The excerpt replay and every automation script start there.
const CAPTURE_ARMED_LOG: &[u8] = b"rig: capture armed";

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
    /// `SET` commands the plan schedules, pre-arm and timed. A dump that ends before the timed
    /// schedule completes leaves `sets_sent` short of this.
    pub sets_planned: usize,
    /// Why the session stopped early, if it did.
    pub error: Option<ControlError>,
}

impl SessionRun {
    /// Pass only when the session ran to `DUMPEND` and the device acknowledged the `FX`, every
    /// `SET` and the `REL`, and rejected nothing. A schedule cut short by an early `DUMPEND` fails.
    pub fn outcome(&self) -> Outcome {
        let o = &self.observed;
        let clean = self.error.is_none()
            && o.fx_acked
            && o.dump_end
            && self.sets_sent == self.sets_planned
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
    /// `BOOT` records read when the session decided the board was fresh; one more means a reset.
    boots_at_start: usize,
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

    /// Release every knob in `knobs`. All are attempted; the first failure is reported.
    fn release(&mut self, knobs: &[u8]) -> Result<(), ControlError> {
        self.released = true;
        let mut first = Ok(());
        for &knob in knobs {
            let sent = self.send(Command::OverrideRelease { knob });
            if first.is_ok() {
                first = sent;
            }
        }
        first
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
        if self.sent_anything && self.observed.boots > self.boots_at_start {
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
        while !self.observed.fresh_boot() {
            if Instant::now() >= boot_deadline {
                return Err(ControlError::NoDevice(
                    "no BOOT record and no early idle stream after attaching: this is not a fresh boot, refusing to send FX"
                        .into(),
                ));
            }
            self.pump()?;
        }

        self.boots_at_start = self.observed.boots;
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
        for &(knob, value) in &timeline.pre {
            self.send_set(knob, value)?;
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
            // A SET after the release would re-engage the device override with nothing left to
            // release it, so a released session never sends again.
            while !self.released && next < timeline.timed.len() && timeline.timed[next].0 <= elapsed
            {
                let (_, knob, value) = timeline.timed[next];
                self.send_set(knob, value)?;
                next += 1;
            }
            let due = timeline.release_at.is_some_and(|r| elapsed >= r);
            if !self.released && next == timeline.timed.len() && due {
                self.release(&timeline.knobs)?;
            }
            if dump_seen_at.is_none() && self.observed.dump_end {
                dump_seen_at = Some(Instant::now());
                // A hold has no scheduled release; the dump ending is the cue. The grace below
                // keeps reading so the acknowledgement lands in the stream.
                if !self.released {
                    self.release(&timeline.knobs)?;
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
                sets_planned: 0,
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
        boots_at_start: 0,
    };
    let sets_planned = timeline.pre.len() + timeline.timed.len();
    let mut error = r.drive(plan, &timeline, opts).err();
    if r.sent_anything && !r.released {
        // Best effort: the failure that got us here is the one worth reporting.
        if let Err(e) = r.release(&timeline.knobs) {
            error.get_or_insert(e);
        }
    }
    SessionRun {
        raw: r.raw,
        observed: r.observed,
        sets_sent: r.sets_sent,
        sets_planned,
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
        /// The dump ends once this many `SET`s have been acknowledged.
        dump_after_sets: Option<usize>,
        sets: usize,
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
                dump_after_sets: None,
                sets: 0,
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
                    Command::OverrideSet { .. } => {
                        self.sets += 1;
                        if self.dump_after_sets == Some(self.sets) {
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
            knob: Some(2),
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
        assert!(sets.len() >= 3);
        // The pre-arm 'from' SET, then the first timed step at the same level.
        assert_eq!(sets[..2], [0, 0]);
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
            sets_planned: 0,
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
    fn dump_ending_before_the_schedule_stops_the_sets() {
        let (mut rig, written) = Rig::new();
        rig.dump_on_arm = true;
        // A grace longer than the schedule gives any late SET time to go out.
        let o = SessionOpts {
            dump_grace: Duration::from_millis(400),
            ..opts(3000)
        };
        let run = run_session(rig, &sweep_plan(), &o);
        let c = cmds(&written);
        assert_eq!(c.last(), Some(&Command::OverrideRelease { knob: 2 }));
        assert_eq!(
            c.iter()
                .filter(|c| matches!(c, Command::OverrideRelease { .. }))
                .count(),
            1
        );
        let sets = c
            .iter()
            .filter(|c| matches!(c, Command::OverrideSet { .. }))
            .count();
        assert_eq!(run.sets_sent, sets);
        assert_eq!(run.observed.set_acks, sets);
        assert!(run.sets_sent < run.sets_planned, "{}", run.summary());
        assert_eq!(run.outcome(), Outcome::Fail, "{}", run.summary());
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
        // Only the pre-arm 'from' SET went out; none of the timed steps.
        let sets = c
            .iter()
            .filter(|c| matches!(c, Command::OverrideSet { .. }))
            .count();
        assert_eq!(sets, 1);
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

    fn observe_bodies(bodies: &[(u32, &[u8])]) -> Observed {
        let mut o = Observed::default();
        for (i, &(t_ms, body)) in bodies.iter().enumerate() {
            let mut d = Decoder::new();
            d.feed(&frame_body(i as u32, t_ms, body), |r| o.observe(r));
        }
        o
    }

    #[test]
    fn arm_time_is_the_log_record_not_the_next_capstat() {
        // The device logs the arming at 2305 ms; the first CAPSTAT that counts a block is a
        // second later. The script's time zero must be the first.
        let o = observe_bodies(&[
            (2205, b"CAPSTAT proto=1 delivered=0 expected=30 overrun=0"),
            (2305, b"rig: capture armed for 10 s (30 blocks)"),
            (3205, b"CAPSTAT proto=1 delivered=2 expected=28 overrun=0"),
        ]);
        assert!(o.armed);
        assert_eq!(o.arm_t_ms, Some(2305));
        let ring_fill = observe_bodies(&[(
            700,
            b"rig: capture armed until the ring is full (30 blocks)",
        )]);
        assert_eq!(ring_fill.arm_t_ms, Some(700));
    }

    #[test]
    fn arm_time_falls_back_to_capstat_when_the_log_record_was_missed() {
        let o = observe_bodies(&[(3205, b"CAPSTAT proto=1 delivered=2 expected=28 overrun=0")]);
        assert!(o.armed);
        assert_eq!(o.arm_t_ms, Some(3205));
    }

    #[test]
    fn no_boot_sends_nothing() {
        let (mut rig, written) = Rig::new();
        rig.boot = false;
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::NoDevice(_))));
        assert!(cmds(&written).is_empty());
    }

    /// A bootloaded start: `BOOT` and the first records were sent before USB connected, so the
    /// stream starts at `first_seq` with device time `t_ms`, optionally already armed.
    fn lost_prefix_rig(
        first_seq: u32,
        t_ms: u32,
        delivered: u32,
    ) -> (Rig, Arc<Mutex<Vec<Command>>>) {
        let (mut rig, written) = Rig::new();
        rig.queue.clear();
        rig.seq = first_seq;
        let body = format!(
            "CAPSTAT proto=1 delivered={delivered} expected=1 overrun=0 max_block_us=1 worst_gap_us=1 audio_exit=0 dumped=0 dropped_full=0"
        );
        let f = frame_body(first_seq, t_ms, body.as_bytes());
        rig.queue.push_back(f);
        (rig, written)
    }

    #[test]
    fn lost_prefix_early_idle_stream_is_a_fresh_boot() {
        let (rig, written) = lost_prefix_rig(5, 203, 0);
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(run.observed.fx_acked, "{:?}", run.error);
        assert!(cmds(&written)
            .iter()
            .any(|c| matches!(c, Command::ReplayFx { .. })));
    }

    #[test]
    fn lost_prefix_already_armed_board_is_refused() {
        let (rig, written) = lost_prefix_rig(5, 203, 7);
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::NoDevice(_))));
        assert!(cmds(&written).is_empty());
    }

    #[test]
    fn lost_prefix_late_attach_is_refused() {
        let (rig, written) = lost_prefix_rig(900, 600_000, 0);
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(matches!(run.error, Some(ControlError::NoDevice(_))));
        assert!(cmds(&written).is_empty());
    }

    #[test]
    fn boot_after_lost_prefix_start_is_a_device_reset() {
        let (mut rig, _w) = lost_prefix_rig(5, 203, 0);
        rig.reset_on_write = Some(1);
        let run = run_session(rig, &sweep_plan(), &opts(3000));
        assert!(
            matches!(run.error, Some(ControlError::DeviceReset)),
            "{:?}",
            run.error
        );
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
        assert_eq!(t.timed[0], (Duration::from_millis(20), 2, 0));
        assert_eq!(t.timed.last().unwrap().2, 0xffff);
        assert_eq!(t.release_at, Some(Duration::from_millis(320)));
        // The 'from' level is applied before the capture arms so the first bucket is settled.
        assert_eq!(t.pre, vec![(2, 0)]);
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
        assert_eq!(t.timed[0], (Duration::from_secs(10), 2, 0x8000));
        assert_eq!(t.timed.last().unwrap().2, 0x8000);
        let peak = t.timed[..1600]
            .iter()
            .chain(&t.timed[1700..3200])
            .map(|&(_, _, v)| v)
            .max();
        assert_eq!(peak, Some(level_to_wire(0.95).unwrap()));
        // Ten samples of 50 ms inside each half-second snap sit 0.25 above the ramp.
        let at = |s: f64| t.timed[(s * 20.0) as usize].2;
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
            .all(|&(_, _, v)| v <= level_to_wire(0.95).unwrap()));
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
                pre: vec![(2, 0xffff)],
                timed: vec![],
                knobs: vec![2],
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

    fn automation_plan(text: &str) -> SessionPlan {
        SessionPlan {
            knob: None,
            motion: Motion::Automation { text: text.into() },
            ..sweep_plan()
        }
    }

    fn wire_of_gain_db(db: f32) -> u16 {
        let c = asperitas_dsp::asperitas::Controls::from_values(&[("gain_db", db)]).unwrap();
        level_to_wire(f64::from(c.knobs[1])).unwrap()
    }

    #[test]
    fn automation_hold_is_every_knob_set_before_arming_and_nothing_timed() {
        let t = automation_plan("0 cutoff_hz=1000 gain_db=-6\n")
            .timeline()
            .unwrap();
        let cutoff = asperitas_dsp::asperitas::Controls::from_values(&[("cutoff_hz", 1000.0)])
            .unwrap()
            .knobs[0];
        assert_eq!(
            t.pre,
            vec![
                (0, level_to_wire(f64::from(cutoff)).unwrap()),
                (1, wire_of_gain_db(-6.0))
            ]
        );
        assert!(t.timed.is_empty());
        assert_eq!(t.knobs, vec![0, 1]);
        assert_eq!(t.release_at, None);
    }

    #[test]
    fn automation_step_is_one_set_at_the_block_start_it_lands_on() {
        // 1 ms is frame 48, inside the block that starts at 32; the update goes out at 64.
        let t = automation_plan("0.001 step gain_db=-12\n")
            .timeline()
            .unwrap();
        let at = Duration::from_secs_f64(64.0 / 48_000.0);
        assert_eq!(t.timed, vec![(at, 1, wire_of_gain_db(-12.0))]);
        // Knobs the script never names start at their defaults, so the board holds them there.
        assert_eq!(t.pre[0].0, 0);
        assert_eq!(t.pre[1], (1, wire_of_gain_db(0.0)));
        assert_eq!(t.release_at, None);
    }

    #[test]
    fn automation_ramp_is_a_set_every_10_ms_ending_on_the_target() {
        let t = automation_plan("0 gain_db=-20\n0.1 ramp gain_db=-10\n")
            .timeline()
            .unwrap();
        assert_eq!(t.pre[1], (1, wire_of_gain_db(-20.0)));
        let offsets: Vec<_> = t.timed.iter().map(|e| e.0).collect();
        assert_eq!(
            offsets,
            (1..=10)
                .map(|i| Duration::from_millis(10 * i))
                .collect::<Vec<_>>()
        );
        assert!(t.timed.iter().all(|e| e.1 == 1));
        let values: Vec<u16> = t.timed.iter().map(|e| e.2).collect();
        assert!(values.windows(2).all(|w| w[0] < w[1]), "{values:?}");
        assert_eq!(*values.last().unwrap(), wire_of_gain_db(-10.0));
        assert_eq!(t.release_at, None);
    }

    fn resonator_plan(text: &str) -> SessionPlan {
        SessionPlan {
            fx_f: FX_RESONATOR,
            ..automation_plan(text)
        }
    }

    fn wire_of_resonator(key: &str, value: f32) -> (u8, u16) {
        let (id, level) = crate::knob::real_to_level(key, f64::from(value)).unwrap();
        (id, level_to_wire(level).unwrap())
    }

    #[test]
    fn resonator_automation_overrides_all_five_knobs_with_real_unit_keys() {
        let t = resonator_plan("0 root_hz=220 damping=0.3\n0.05 ramp damping=0.8\n")
            .timeline()
            .unwrap();
        assert_eq!(t.knobs, vec![0, 1, 2, 3, 4]);
        assert_eq!(t.pre.len(), 5);
        assert_eq!(t.pre[0], wire_of_resonator("root_hz", 220.0));
        assert_eq!(t.pre[2], wire_of_resonator("damping", 0.3));
        // Untouched knobs start from their table default.
        assert_eq!(t.pre[1], wire_of_resonator("structure", 0.0));
        assert!(t.timed.iter().all(|e| e.1 == 2));
        assert_eq!(
            t.timed.last().unwrap().2,
            wire_of_resonator("damping", 0.8).1
        );
    }

    #[test]
    fn resonator_automation_refuses_a_patch_key_a_bad_value_and_a_ramped_preset() {
        for text in [
            "0 cutoff_hz=1000\n",
            "0 damping=3\n",
            "0 excitation_preset=1.5\n",
            "0 excitation_preset=1\n1 ramp excitation_preset=2\n",
        ] {
            let e = resonator_plan(text).timeline().unwrap_err().to_string();
            let key = text.split(['=', ' ']).nth(1).unwrap();
            assert!(e.contains(key), "{text:?}: {e}");
        }
    }

    #[test]
    fn resonator_sidecar_ends_at_its_own_last_update() {
        let side = resonator_plan("0 damping=0.1\n0.1 step damping=0.9\n").sidecar(None, None);
        assert_eq!(side.fx_f, FX_RESONATOR);
        assert!((side.duration_s - 0.1).abs() < 1e-9, "{}", side.duration_s);
    }

    #[test]
    fn malformed_automation_or_knob_mismatch_touches_nothing() {
        let cases = [
            automation_plan("0 bogus=1\n"),
            automation_plan("0 gain_db=20\n"),
            automation_plan("2 gain_db=0\n1 gain_db=-1\n"),
            SessionPlan {
                knob: Some(0),
                ..automation_plan("0 gain_db=0\n")
            },
            SessionPlan {
                knob: None,
                ..SessionPlan {
                    motion: Motion::Hold { level: 0.5 },
                    ..sweep_plan()
                }
            },
        ];
        for plan in cases {
            let (rig, written) = Rig::new();
            let run = run_session(rig, &plan, &opts(3000));
            assert!(
                matches!(run.error, Some(ControlError::Usage(_))),
                "{plan:?}: {:?}",
                run.error
            );
            assert!(cmds(&written).is_empty(), "{plan:?}");
            assert!(run.raw.is_empty());
        }
    }

    #[test]
    fn automation_session_sets_every_knob_and_releases_them_all_last() {
        let plan = automation_plan("0 cutoff_hz=1000 gain_db=-20\n0.05 ramp gain_db=-10\n");
        let (mut rig, written) = Rig::new();
        // The board ends its dump after the last ramp step, so the release is the dump's cue.
        rig.dump_after_sets = Some(7);
        let run = run_session(rig, &plan, &opts(3000));
        assert!(run.error.is_none(), "{:?}", run.error);
        let c = cmds(&written);
        assert_eq!(c[0], Command::ReplayFx { fx: 1, value: 0 });
        let sets: Vec<(u8, u16)> = c
            .iter()
            .filter_map(|c| match c {
                Command::OverrideSet { knob, value } => Some((*knob, *value)),
                _ => None,
            })
            .collect();
        // Two pre-arm SETs, then the five ramp steps on the gain knob only.
        assert_eq!(sets.len(), 7, "{sets:?}");
        assert_eq!((sets[0].0, sets[1].0), (0, 1));
        assert!(sets[2..].iter().all(|s| s.0 == 1));
        assert_eq!(sets.last().unwrap().1, wire_of_gain_db(-10.0));
        assert_eq!(
            c[c.len() - 2..],
            [
                Command::OverrideRelease { knob: 0 },
                Command::OverrideRelease { knob: 1 }
            ]
        );
        assert_eq!(run.outcome(), Outcome::Pass, "{}", run.summary());
    }

    #[test]
    fn automation_sidecar_carries_the_script_and_names_no_knob() {
        let text = "0 gain_db=-20\n0.05 ramp gain_db=-10\n";
        let side = automation_plan(text).sidecar(Some(1), None);
        assert_eq!(side.mode, Mode::Automation);
        assert_eq!(side.knob, None);
        assert_eq!(side.automation.as_deref(), Some(text));
        assert!((side.duration_s - 0.05).abs() < 1e-9);
        assert_eq!(Sidecar::parse(&side.render()), Ok(side));
        // The other motions keep their sidecars free of any automation.
        assert_eq!(sweep_plan().sidecar(None, None).automation, None);
    }
}
