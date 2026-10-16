//! The `asperitas-measure analyze` command: load a committed capture, judge it, print one JSON
//! document, and map the verdict to the exit status `asperitas-rig` uses.
//!
//! ```text
//! asperitas-measure analyze <capture.console.zst> [--stimulus sine|sweep|pulse]
//! asperitas-measure analyze <capture.console.zst> --excerpt <clip.wav> \
//!     [--fx bypass|gain|lowpass] [--fx-value <0..65535 | 0xHEX>]
//! asperitas-measure analyze <capture.console.zst> --sweep FROM:TO --sweep-kind gain|lowpass \
//!     [--sweep-start-s S] [--sweep-duration-s D]
//! ```
//!
//! The sweep form judges a knob sweep from `asperitas-rig override --sweep FROM:TO` (knob levels
//! `0..=1`): the response curve against knob value, its direction, monotonicity and flat regions
//! (see [`crate::sweep`]). `--sweep-start-s` is the capture time of the first `SET` (default 0) and
//! `--sweep-duration-s` the sweep length (default: the rest of the capture); the capture does not
//! record either, and the rig's final ack grace follows the sweep, so pass the duration the rig was
//! given when it is known.
//!
//! The first form judges a steady-stimulus loopback capture. Only the sine is judged against the
//! TASK-034 baseline (level, silence, discontinuities, spurs, parameter steps); the sweep and
//! pulse are reported and checked for alignment stability only, because the baseline limits are
//! defined for the sine. The second form judges an excerpt replay against the `asperitas-cli`
//! reference run at the same `CTL FX` selection (`--fx`, `--fx-value` take the wire values sent to
//! the board; default bypass). Exit 0 passes, 1 is a failed verdict, 2 is a usage, load or I/O error.

use std::io::Write;

use asperitas_dsp::stimulus::{ExponentialSweepParams, PulseTrainParams, SineParams};
use serde_json::{json, Value};

use crate::align::{align, assert_stable};
use crate::capture::Capture;
use crate::compare::{compare, FxSetting, Reference};
use crate::metrics::analyze;
use crate::param_step::detect_param_steps;
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
[--stimulus sine|sweep|pulse | --excerpt <clip.wav> [--fx bypass|gain|lowpass] [--fx-value <0..65535|0xHEX>] \
| --sweep FROM:TO --sweep-kind gain|lowpass [--sweep-start-s S] [--sweep-duration-s D]]";

enum Mode {
    Stimulus(String),
    Excerpt(String, FxSetting),
    Sweep(SweepArgs),
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
    let (path, mode) = match parse(args) {
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
    let result = match mode {
        Mode::Stimulus(name) => judge_stimulus(&capture, &name),
        Mode::Excerpt(wav, fx) => judge_excerpt(&capture, &wav, fx),
        Mode::Sweep(args) => Ok(judge_sweep_capture(&capture, &args)),
    };
    match result {
        Ok((report, failures)) => {
            let mut report = report;
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

fn parse(args: &[String]) -> Result<(String, Mode), String> {
    let mut it = args.iter();
    match it.next().map(String::as_str) {
        Some("analyze") => {}
        Some(other) => return Err(format!("unknown subcommand {other:?}")),
        None => return Err("missing subcommand".into()),
    }
    let mut path = None;
    let mut stimulus = None;
    let mut excerpt = None;
    let mut fx_name = None;
    let mut fx_value = None;
    let mut sweep = None;
    let mut sweep_kind = None;
    let mut sweep_start = None;
    let mut sweep_duration = None;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--stimulus" => stimulus = Some(it.next().ok_or("--stimulus needs a value")?.clone()),
            "--excerpt" => excerpt = Some(it.next().ok_or("--excerpt needs a value")?.clone()),
            "--fx" => fx_name = Some(it.next().ok_or("--fx needs a value")?.clone()),
            "--fx-value" => {
                fx_value = Some(parse_wire_value(
                    it.next().ok_or("--fx-value needs a value")?,
                )?)
            }
            "--sweep" => sweep = Some(it.next().ok_or("--sweep needs FROM:TO")?.clone()),
            "--sweep-kind" => {
                sweep_kind = Some(it.next().ok_or("--sweep-kind needs a value")?.clone())
            }
            "--sweep-start-s" => sweep_start = Some(parse_seconds("--sweep-start-s", it.next())?),
            "--sweep-duration-s" => {
                sweep_duration = Some(parse_seconds("--sweep-duration-s", it.next())?)
            }
            s if s.starts_with("--") => return Err(format!("unknown flag {s}")),
            _ if path.is_none() => path = Some(a.clone()),
            _ => return Err(format!("unexpected argument {a:?}")),
        }
    }
    let path = path.ok_or("missing capture path")?;
    if excerpt.is_none() && (fx_name.is_some() || fx_value.is_some()) {
        return Err("--fx and --fx-value need --excerpt".into());
    }
    if let Some(range) = sweep {
        if stimulus.is_some() || excerpt.is_some() || fx_name.is_some() || fx_value.is_some() {
            return Err("--sweep is exclusive with --stimulus, --excerpt, --fx".into());
        }
        let (from, to) = range
            .split_once(':')
            .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
            .filter(|(a, b)| (0.0..=1.0).contains(a) && (0.0..=1.0).contains(b))
            .ok_or_else(|| format!("--sweep wants FROM:TO knob levels in 0..=1, got {range:?}"))?;
        let kind_name = sweep_kind.ok_or("--sweep needs --sweep-kind gain|lowpass")?;
        let kind = SweepKind::from_name(&kind_name)
            .ok_or_else(|| format!("unknown --sweep-kind {kind_name:?} (gain, lowpass)"))?;
        return Ok((
            path,
            Mode::Sweep(SweepArgs {
                kind,
                from,
                to,
                start_s: sweep_start.unwrap_or(0.0),
                duration_s: sweep_duration,
            }),
        ));
    }
    if sweep_kind.is_some() || sweep_start.is_some() || sweep_duration.is_some() {
        return Err("--sweep-kind, --sweep-start-s and --sweep-duration-s need --sweep".into());
    }
    let mode = match (stimulus, excerpt) {
        (Some(_), Some(_)) => return Err("--stimulus and --excerpt are exclusive".into()),
        (None, Some(w)) => {
            let name = fx_name.unwrap_or_else(|| "bypass".into());
            let fx = FxSetting::from_name(&name, fx_value.unwrap_or(0))
                .ok_or_else(|| format!("unknown --fx {name:?} (bypass, gain, lowpass)"))?;
            Mode::Excerpt(w, fx)
        }
        (Some(s), None) => Mode::Stimulus(s),
        (None, None) => Mode::Stimulus("sine".into()),
    };
    Ok((path, mode))
}

fn parse_seconds(flag: &str, v: Option<&String>) -> Result<f64, String> {
    v.ok_or_else(|| format!("{flag} needs a value"))?
        .parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .ok_or_else(|| format!("{flag} wants non-negative seconds"))
}

/// A `CTL FX v=` value: decimal or `0x` hex, `0..=0xFFFF`.
fn parse_wire_value(s: &str) -> Result<u16, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u16::from_str_radix(h, 16),
        None => s.parse::<u16>(),
    };
    parsed.map_err(|_| format!("--fx-value {s:?} is not 0..=65535 or 0x0000..=0xFFFF"))
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
