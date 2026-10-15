//! The `asperitas-measure analyze` command: load a committed capture, judge it, print one JSON
//! document, and map the verdict to the exit status `asperitas-rig` uses.
//!
//! ```text
//! asperitas-measure analyze <capture.console.zst> [--stimulus sine|sweep|pulse]
//! asperitas-measure analyze <capture.console.zst> --excerpt <clip.wav>
//! ```
//!
//! The first form judges a steady-stimulus loopback capture. Only the sine is judged against the
//! TASK-034 baseline (level, silence, discontinuities, spurs, parameter steps); the sweep and
//! pulse are reported and checked for alignment stability only, because the baseline limits are
//! defined for the sine. The second form judges an excerpt replay against the `asperitas-cli`
//! reference. Exit 0 passes, 1 is a failed verdict, 2 is a usage, load or I/O error.

use std::io::Write;

use asperitas_dsp::stimulus::{ExponentialSweepParams, PulseTrainParams, SineParams};
use serde_json::{json, Value};

use crate::align::{align, assert_stable};
use crate::capture::Capture;
use crate::compare::{compare, Reference};
use crate::metrics::analyze;
use crate::param_step::detect_param_steps;
use crate::stimulus::Stimulus;
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
[--stimulus sine|sweep|pulse | --excerpt <clip.wav>]";

enum Mode {
    Stimulus(String),
    Excerpt(String),
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
        Mode::Excerpt(wav) => judge_excerpt(&capture, &wav),
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
    while let Some(a) = it.next() {
        match a.as_str() {
            "--stimulus" => stimulus = Some(it.next().ok_or("--stimulus needs a value")?.clone()),
            "--excerpt" => excerpt = Some(it.next().ok_or("--excerpt needs a value")?.clone()),
            s if s.starts_with("--") => return Err(format!("unknown flag {s}")),
            _ if path.is_none() => path = Some(a.clone()),
            _ => return Err(format!("unexpected argument {a:?}")),
        }
    }
    let path = path.ok_or("missing capture path")?;
    let mode = match (stimulus, excerpt) {
        (Some(_), Some(_)) => return Err("--stimulus and --excerpt are exclusive".into()),
        (None, Some(w)) => Mode::Excerpt(w),
        (Some(s), None) => Mode::Stimulus(s),
        (None, None) => Mode::Stimulus("sine".into()),
    };
    Ok((path, mode))
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

fn judge_excerpt(capture: &Capture, wav: &str) -> Result<(Value, Vec<String>), String> {
    let raw = std::fs::read(wav).map_err(|e| format!("{wav}: {e}"))?;
    let reference = Reference::from_excerpt_wav(&raw).map_err(|e| format!("{wav}: {e}"))?;
    match compare(capture, &reference) {
        Ok(c) => {
            let failures = c.check().err().map(|e| e.to_string()).into_iter().collect();
            Ok((json!({"excerpt": wav, "comparison": c}), failures))
        }
        Err(e) => Ok((json!({"excerpt": wav}), vec![format!("comparison: {e}")])),
    }
}
