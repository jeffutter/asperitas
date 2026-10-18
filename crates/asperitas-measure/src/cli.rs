//! The `asperitas-measure analyze` command: load a committed capture, judge it, print one JSON
//! document, and map the verdict to the exit status `asperitas-rig` uses.
//!
//! ```text
//! asperitas-measure analyze <capture.console.zst> [--stimulus sine|sweep|pulse]
//! asperitas-measure analyze <capture.console.zst> --excerpt <clip.wav>
//! asperitas-measure analyze <capture.console.zst> [--expect-fail]
//! ```
//!
//! A capture recorded by `asperitas-rig session` has a sidecar beside it
//! ([`asperitas_host::session::sidecar_path`]) naming the effect selection, the knob, the motion
//! and when it started. When the sidecar exists it decides what is judged, and no other mode flag
//! is accepted (exit 2):
//!
//! - a sweep session is judged as a knob sweep ([`crate::sweep`]): the response curve against knob
//!   value, its direction, monotonicity and flat regions. Knob 0 is the low-pass cutoff and knob 1
//!   the gain;
//! - a soak or hold session is judged as a long capture taken while a knob moves, or is held
//!   ([`crate::soak`]): silence, discontinuity and parameter-step candidates over the whole run,
//!   attributed to the motion window or to the time outside it. A hold has no motion window.
//!   `--expect-fail` inverts the verdict for an unsmoothed build: exit 0 only when the step
//!   detector fired (and no dropout is present), exit 1 when the capture unexpectedly passes;
//! - with `--excerpt`, the replay is judged against the `asperitas-cli` reference run at the
//!   sidecar's effect selection, and for a hold at its knob level. A sweep or soak moves the knob
//!   during the capture, which the reference cannot follow, so that combination is a usage error.
//!
//! The motion starts at [`Sidecar::sweep_start_s`] into the capture: the device clock's span from
//! the capture arming to the first `SET` when both were recorded, the host's planned delay
//! otherwise.
//!
//! Without a sidecar the capture is a steady-stimulus loopback. Only the sine is judged against
//! the baseline in [`crate::thresholds`] (level, silence, discontinuities, spurs, parameter
//! steps); the sweep and pulse are reported and checked for alignment stability only, because the
//! baseline limits are defined for the sine. `--excerpt` then judges an excerpt replay at the
//! device's default effect (bypass). Exit 0 passes, 1 is a failed verdict, 2 is a usage, load or
//! I/O error.

use std::io::Write;

use std::path::Path;

use asperitas_dsp::stimulus::{ExponentialSweepParams, PulseTrainParams, SineParams};
use asperitas_host::session::{sidecar_path, Mode as Motion, Sidecar};
use serde_json::{json, Value};

use crate::align::{align, assert_stable};
use crate::capture::Capture;
use crate::compare::{compare, FxSetting, Reference};
use crate::metrics::analyze;
use crate::param_step::detect_param_steps;
use crate::soak::{judge_soak, SoakSpec};
use crate::stimulus::Stimulus;
use crate::sweep::{judge_sweep, SweepKind, SweepSpec};
use crate::thresholds;

/// Process exit status, numbered as `asperitas-rig` numbers its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Error,
}

impl Outcome {
    pub fn code(self) -> u8 {
        match self {
            Outcome::Pass => 0,
            Outcome::Fail => 1,
            Outcome::Error => 2,
        }
    }
}

const USAGE: &str = "usage: asperitas-measure analyze <capture.console.zst> \
[--stimulus sine|sweep|pulse | --excerpt <clip.wav> | --expect-fail]\n\
a sidecar written by `asperitas-rig session` beside the capture decides the mode; \
with one, only --excerpt (hold sessions) and --expect-fail (soak and hold) are accepted";

/// What the command line asked for; the sidecar, if any, decides the rest.
struct Flags {
    stimulus: Option<String>,
    excerpt: Option<String>,
    expect_fail: bool,
}

enum Mode {
    Stimulus(String),
    Excerpt(String, FxSetting),
    Sweep(SweepArgs),
    Soak(SoakArgs),
}

struct SoakArgs {
    from: f64,
    to: f64,
    start_s: f64,
    duration_s: Option<f64>,
    expect_fail: bool,
}

struct SweepArgs {
    kind: SweepKind,
    from: f64,
    to: f64,
    start_s: f64,
    duration_s: Option<f64>,
}

/// Run the command with `args` (without the program name). The JSON report goes to `out`;
/// diagnostics go to `err`.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Outcome {
    let (path, mode) = match parse(args).and_then(|(path, flags)| {
        let mode = resolve(&path, flags)?;
        Ok((path, mode))
    }) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(err, "error: {e}\n{USAGE}");
            return Outcome::Error;
        }
    };
    let capture = match Capture::from_console_zst(&path) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "error: {path}: {e}");
            return Outcome::Error;
        }
    };
    let expect_fail = matches!(&mode, Mode::Soak(a) if a.expect_fail);
    let result = match mode {
        Mode::Stimulus(name) => judge_stimulus(&capture, &name),
        Mode::Excerpt(wav, fx) => judge_excerpt(&capture, &wav, fx),
        Mode::Sweep(args) => Ok(judge_sweep_capture(&capture, &args)),
        Mode::Soak(args) => judge_soak_capture(&capture, &args),
    };
    match result {
        Ok((report, failures)) => {
            let mut report = report;
            let failures = if expect_fail {
                invert_for_expected_failure(&mut report, failures)
            } else {
                failures
            };
            report["capture"] = json!(path);
            report["pass"] = json!(failures.is_empty());
            report["failures"] = json!(failures);
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&report).unwrap());
            for f in &failures {
                let _ = writeln!(err, "FAIL: {f}");
            }
            if failures.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail
            }
        }
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            Outcome::Error
        }
    }
}

fn parse(args: &[String]) -> Result<(String, Flags), String> {
    let mut it = args.iter();
    match it.next().map(String::as_str) {
        Some("analyze") => {}
        Some(other) => return Err(format!("unknown subcommand {other:?}")),
        None => return Err("missing subcommand".into()),
    }
    let mut path = None;
    let mut flags = Flags {
        stimulus: None,
        excerpt: None,
        expect_fail: false,
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--stimulus" => {
                flags.stimulus = Some(it.next().ok_or("--stimulus needs a value")?.clone())
            }
            "--excerpt" => {
                flags.excerpt = Some(it.next().ok_or("--excerpt needs a value")?.clone())
            }
            "--expect-fail" => flags.expect_fail = true,
            s if s.starts_with("--") => return Err(format!("unknown flag {s}")),
            _ if path.is_none() => path = Some(a.clone()),
            _ => return Err(format!("unexpected argument {a:?}")),
        }
    }
    Ok((path.ok_or("missing capture path")?, flags))
}

/// Choose what to judge from the flags and the sidecar beside `path`, if there is one.
fn resolve(path: &str, flags: Flags) -> Result<Mode, String> {
    let side_path = sidecar_path(Path::new(path));
    let sidecar = match std::fs::read_to_string(&side_path) {
        Ok(text) => {
            Some(Sidecar::parse(&text).map_err(|e| format!("{}: {e}", side_path.display()))?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", side_path.display())),
    };
    let Some(side) = sidecar else {
        if flags.expect_fail {
            return Err("--expect-fail needs a session sidecar beside the capture".into());
        }
        return match (flags.stimulus, flags.excerpt) {
            (Some(_), Some(_)) => Err("--stimulus and --excerpt are exclusive".into()),
            (None, Some(wav)) => Ok(Mode::Excerpt(wav, FxSetting::BYPASS)),
            (Some(s), None) => Ok(Mode::Stimulus(s)),
            (None, None) => Ok(Mode::Stimulus("sine".into())),
        };
    };
    if flags.stimulus.is_some() {
        return Err("--stimulus does not apply to a capture with a session sidecar".into());
    }
    let held = side.mode == Motion::Hold;
    if let Some(wav) = flags.excerpt {
        if flags.expect_fail {
            return Err("--excerpt and --expect-fail are exclusive".into());
        }
        if !held {
            return Err(
                "--excerpt needs a hold session: the reference cannot follow a moving knob".into(),
            );
        }
        let mut fx = FxSetting::from_code(side.fx_f);
        let wire = (side.from * f64::from(u16::MAX)).round() as u16;
        fx = fx
            .with_knob(usize::from(side.knob), wire)
            .ok_or_else(|| format!("sidecar knob {} is not a knob of the patch", side.knob))?;
        return Ok(Mode::Excerpt(wav, fx));
    }
    let start_s = side.sweep_start_s();
    match side.mode {
        Motion::Sweep => {
            if flags.expect_fail {
                return Err("--expect-fail needs a soak or hold session".into());
            }
            let kind = match side.knob {
                0 => SweepKind::LowPass,
                1 => SweepKind::Gain,
                k => return Err(format!("sidecar knob {k} has no sweep judgement (0, 1)")),
            };
            Ok(Mode::Sweep(SweepArgs {
                kind,
                from: side.from,
                to: side.to,
                start_s,
                duration_s: Some(side.duration_s),
            }))
        }
        Motion::Soak | Motion::Hold => Ok(Mode::Soak(SoakArgs {
            from: side.from,
            to: side.to,
            start_s: if held { 0.0 } else { start_s },
            duration_s: (!held).then_some(side.duration_s),
            expect_fail: flags.expect_fail,
        })),
    }
}

/// `Ok((report, failures))` when the analysis ran; `Err` only for conditions that are not a
/// verdict about the capture (an unknown stimulus name, an unreadable clip).
fn judge_stimulus(capture: &Capture, name: &str) -> Result<(Value, Vec<String>), String> {
    let (stimulus, judged) = match name {
        "sine" => (Stimulus::Sine(SineParams::default()), true),
        "sweep" => (Stimulus::Sweep(ExponentialSweepParams::default()), false),
        "pulse" => (Stimulus::Pulse(PulseTrainParams::default()), false),
        other => return Err(format!("unknown stimulus {other:?} (sine, sweep, pulse)")),
    };
    let rendered = stimulus.render(capture.samples.len());
    let mut failures = Vec::new();

    let alignment = match align(capture, &rendered) {
        Ok(a) => a,
        Err(e) => {
            // Too short to align is a property of the capture, so a verdict, not a usage error.
            failures.push(format!("alignment: {e}"));
            return Ok((json!({"stimulus": name}), failures));
        }
    };
    if let Err(e) = assert_stable(&alignment) {
        failures.push(format!("alignment: {e}"));
    }
    let metrics = analyze(capture, &rendered, &alignment);
    let normalized: Vec<f32> = capture
        .samples
        .iter()
        .map(|&s| f32::from(s) / 32768.0)
        .collect();
    let steps = detect_param_steps(&normalized);

    if judged {
        if metrics.level.deviation_db.abs() > thresholds::LEVEL_DEVIATION_TOL_DB {
            failures.push(format!(
                "level deviates {:.3} dB from baseline (limit {} dB)",
                metrics.level.deviation_db,
                thresholds::LEVEL_DEVIATION_TOL_DB
            ));
        }
        if metrics.silence.run_count > 0 {
            failures.push(format!("{} silent run(s)", metrics.silence.run_count));
        }
        if metrics.discontinuities.count > 0 {
            failures.push(format!(
                "{} discontinuity candidate(s)",
                metrics.discontinuities.count
            ));
        }
        if metrics.worst_spur_dbc() > thresholds::SPUR_MAX_DBC {
            failures.push(format!(
                "worst spur {:.1} dBc > {} dBc",
                metrics.worst_spur_dbc(),
                thresholds::SPUR_MAX_DBC
            ));
        }
        if steps.count > 0 {
            failures.push(format!("{} parameter step(s)", steps.count));
        }
    }

    let report = json!({
        "stimulus": name,
        "judged_against_baseline": judged,
        "alignment": {
            "windows": alignment.delays.len(),
            "median_delay_samples": alignment.median(),
        },
        "metrics": metrics,
        "param_steps": steps,
    });
    Ok((report, failures))
}

fn judge_excerpt(
    capture: &Capture,
    wav: &str,
    fx: FxSetting,
) -> Result<(Value, Vec<String>), String> {
    let raw = std::fs::read(wav).map_err(|e| format!("{wav}: {e}"))?;
    let reference = Reference::from_excerpt_wav_fx(&raw, fx).map_err(|e| format!("{wav}: {e}"))?;
    match compare(capture, &reference) {
        Ok(c) => {
            let failures = c.check().err().map(|e| e.to_string()).into_iter().collect();
            Ok((json!({"excerpt": wav, "fx": fx, "comparison": c}), failures))
        }
        Err(e) => Ok((
            json!({"excerpt": wav, "fx": fx}),
            vec![format!("comparison: {e}")],
        )),
    }
}

fn judge_sweep_capture(capture: &Capture, args: &SweepArgs) -> (Value, Vec<String>) {
    let rate = f64::from(capture.sample_rate_hz);
    let normalized: Vec<f32> = capture
        .samples
        .iter()
        .map(|&s| f32::from(s) / 32768.0)
        .collect();
    let duration_s = args
        .duration_s
        .unwrap_or(normalized.len() as f64 / rate - args.start_s);
    let spec = SweepSpec {
        kind: args.kind,
        from: args.from,
        to: args.to,
        start_s: args.start_s,
        duration_s,
    };
    match judge_sweep(&normalized, capture.sample_rate_hz, &spec) {
        Ok(r) => {
            let failures = r.failures();
            (
                json!({"sweep": {"from": args.from, "to": args.to, "start_s": args.start_s, "duration_s": duration_s}, "response": r}),
                failures,
            )
        }
        Err(e) => (
            json!({"sweep": {"from": args.from, "to": args.to}}),
            vec![format!("sweep: {e}")],
        ),
    }
}

fn judge_soak_capture(capture: &Capture, args: &SoakArgs) -> Result<(Value, Vec<String>), String> {
    let stimulus = Stimulus::Sine(SineParams::default());
    let rate = f64::from(capture.sample_rate_hz);
    let duration_s = args
        .duration_s
        .unwrap_or(capture.samples.len() as f64 / rate - args.start_s)
        .max(0.0);
    let spec = SoakSpec {
        from: args.from,
        to: args.to,
        start_s: args.start_s,
        duration_s,
    };
    let motion = json!({"from": args.from, "to": args.to, "start_s": args.start_s, "duration_s": duration_s});
    match judge_soak(capture, &stimulus, &spec) {
        Ok(r) => {
            let failures = r.failures.clone();
            Ok((
                json!({"soak": motion, "expected_failure_observed": r.expected_failure_observed(), "result": r}),
                failures,
            ))
        }
        Err(e) => Ok((json!({"soak": motion}), vec![format!("soak: {e}")])),
    }
}

/// `--expect-fail`: the detectors' findings move to `detector_failures` as evidence, and the
/// verdict is whether the soak showed the step detector fired (and no dropout muddied it).
fn invert_for_expected_failure(report: &mut Value, detector_failures: Vec<String>) -> Vec<String> {
    report["detector_failures"] = json!(detector_failures);
    report["expect_failure"] = json!(true);
    if report["expected_failure_observed"] == json!(true) {
        Vec::new()
    } else {
        vec!["expected the step detector to fire on its own, but it did not".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expect_fail_passes_only_when_the_step_detector_fired() {
        let mut fired = json!({"expected_failure_observed": true});
        assert!(invert_for_expected_failure(&mut fired, vec!["3 steps".into()]).is_empty());
        assert_eq!(fired["detector_failures"][0], "3 steps");

        let mut quiet = json!({"expected_failure_observed": false});
        assert_eq!(invert_for_expected_failure(&mut quiet, Vec::new()).len(), 1);
        let mut errored = json!({});
        assert_eq!(
            invert_for_expected_failure(&mut errored, vec!["x".into()]).len(),
            1
        );
    }
}
