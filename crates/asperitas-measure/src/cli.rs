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
//! The motion starts at [`Sidecar::motion_start_s`] into the capture: the device clock's span from
//! the capture arming to the first `SET` when both were recorded, the host's planned delay
//! otherwise.
//!
//! Without a sidecar the capture is a steady-stimulus loopback judged by [`crate::baseline`]
//! (the sine by default). `--excerpt` then judges an excerpt replay at the device's default
//! effect (bypass). Exit 0 passes, 1 is a failed verdict, 2 is a usage, load or I/O error.

use std::io::Write;
use std::path::Path;

use asperitas_dsp::stimulus::SineParams;
use asperitas_host::outcome::Outcome;
use asperitas_host::session::{sidecar_path, Mode as Motion, Sidecar};
use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::baseline::{judge_baseline, StimulusKind};
use crate::capture::Capture;
use crate::compare::{compare, FxSetting, Reference};
use crate::soak::{judge_soak, SoakSpec};
use crate::stimulus::Stimulus;
use crate::sweep::{judge_sweep, SweepKind, SweepSpec};

/// Judge rig loopback captures. A session sidecar beside the capture, written by
/// `asperitas-rig session`, decides what is judged; with one, only `--excerpt` (hold sessions)
/// and `--expect-fail` (soak and hold) are accepted.
#[derive(Parser)]
#[command(name = "asperitas-measure", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Load a committed capture, judge it and print one JSON report.
    Analyze(AnalyzeArgs),
}

#[derive(Args)]
struct AnalyzeArgs {
    /// The `.console.zst` capture.
    path: String,
    /// The generator a sidecar-less capture holds (default sine).
    #[arg(long, value_enum, conflicts_with = "excerpt")]
    stimulus: Option<StimulusKind>,
    /// Judge an excerpt replay against the `asperitas-cli` reference of this clip.
    #[arg(long, value_name = "CLIP.WAV", conflicts_with = "expect_fail")]
    excerpt: Option<String>,
    /// Invert the verdict for an unsmoothed build: pass only when the step detector fired.
    #[arg(long)]
    expect_fail: bool,
}

enum Mode {
    Baseline(StimulusKind),
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

/// A judged capture: the JSON report and the reasons it fails.
struct Judged {
    report: Value,
    failures: Vec<String>,
    /// For a soak: whether the step detector fired with no dropout (see
    /// [`crate::soak::SoakReport::expected_failure_observed`]). False for every other mode and
    /// for a soak that could not be measured.
    expected_failure_observed: bool,
}

impl Judged {
    fn new(report: Value, failures: Vec<String>) -> Judged {
        Judged {
            report,
            failures,
            expected_failure_observed: false,
        }
    }
}

/// Run the command with `args` (without the program name). The JSON report goes to `out`;
/// diagnostics go to `err`.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Outcome {
    let cli = match Cli::try_parse_from(
        std::iter::once("asperitas-measure").chain(args.iter().map(String::as_str)),
    ) {
        Ok(cli) => cli,
        Err(e) => {
            // `--help` and `--version` are requests, not usage errors.
            return if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = write!(out, "{e}");
                Outcome::Pass
            } else {
                let _ = write!(err, "{e}");
                Outcome::Error
            };
        }
    };
    let Command::Analyze(args) = cli.command;
    let mode = match resolve(&args) {
        Ok(mode) => mode,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return Outcome::Error;
        }
    };
    let capture = match Capture::from_console_zst(&args.path) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "error: {}: {e}", args.path);
            return Outcome::Error;
        }
    };
    let expect_fail = matches!(&mode, Mode::Soak(a) if a.expect_fail);
    let judged = match mode {
        Mode::Baseline(kind) => Ok(judge_baseline_capture(&capture, kind)),
        Mode::Excerpt(wav, fx) => judge_excerpt(&capture, &wav, fx),
        Mode::Sweep(sweep) => Ok(judge_sweep_capture(&capture, &sweep)),
        Mode::Soak(soak) => Ok(judge_soak_capture(&capture, &soak)),
    };
    let Judged {
        mut report,
        failures,
        expected_failure_observed,
    } = match judged {
        Ok(j) => j,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return Outcome::Error;
        }
    };
    let failures = if expect_fail {
        invert_for_expected_failure(&mut report, failures, expected_failure_observed)
    } else {
        failures
    };
    report["capture"] = json!(args.path);
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

/// Choose what to judge from the flags and the sidecar beside the capture, if there is one.
/// clap has already rejected `--stimulus` with `--excerpt` and `--excerpt` with `--expect-fail`;
/// what remains depends on the sidecar.
fn resolve(args: &AnalyzeArgs) -> Result<Mode, String> {
    let side_path = sidecar_path(Path::new(&args.path));
    let sidecar = match std::fs::read_to_string(&side_path) {
        Ok(text) => {
            Some(Sidecar::parse(&text).map_err(|e| format!("{}: {e}", side_path.display()))?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", side_path.display())),
    };
    let Some(side) = sidecar else {
        if args.expect_fail {
            return Err("--expect-fail needs a session sidecar beside the capture".into());
        }
        return Ok(match &args.excerpt {
            Some(wav) => Mode::Excerpt(wav.clone(), FxSetting::BYPASS),
            None => Mode::Baseline(args.stimulus.unwrap_or(StimulusKind::Sine)),
        });
    };
    if args.stimulus.is_some() {
        return Err("--stimulus does not apply to a capture with a session sidecar".into());
    }
    let held = side.mode == Motion::Hold;
    if let Some(wav) = &args.excerpt {
        if !held {
            return Err(
                "--excerpt needs a hold session: the reference cannot follow a moving knob".into(),
            );
        }
        let mut fx = FxSetting::from_code(side.fx_f);
        let wire = asperitas_wire::control::level_to_wire(side.from)
            .ok_or_else(|| format!("sidecar level {} is outside 0..=1", side.from))?;
        fx = fx
            .with_knob(usize::from(side.knob), wire)
            .ok_or_else(|| format!("sidecar knob {} is not a knob of the patch", side.knob))?;
        return Ok(Mode::Excerpt(wav.clone(), fx));
    }
    let start_s = side.motion_start_s();
    match side.mode {
        Motion::Sweep => {
            if args.expect_fail {
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
            expect_fail: args.expect_fail,
        })),
    }
}

fn judge_baseline_capture(capture: &Capture, kind: StimulusKind) -> Judged {
    let report = judge_baseline(capture, kind);
    let value = serde_json::to_value(&report).expect("a baseline report serializes");
    Judged::new(value, report.failures)
}

/// `Err` only for conditions that are not a verdict about the capture (an unreadable clip).
fn judge_excerpt(capture: &Capture, wav: &str, fx: FxSetting) -> Result<Judged, String> {
    let raw = std::fs::read(wav).map_err(|e| format!("{wav}: {e}"))?;
    let reference = Reference::from_excerpt_wav_fx(&raw, fx).map_err(|e| format!("{wav}: {e}"))?;
    Ok(match compare(capture, &reference) {
        Ok(c) => {
            let failures = c.check().err().map(|e| e.to_string()).into_iter().collect();
            Judged::new(json!({"excerpt": wav, "fx": fx, "comparison": c}), failures)
        }
        Err(e) => Judged::new(
            json!({"excerpt": wav, "fx": fx}),
            vec![format!("comparison: {e}")],
        ),
    })
}

fn judge_sweep_capture(capture: &Capture, args: &SweepArgs) -> Judged {
    let rate = f64::from(capture.sample_rate_hz);
    let normalized = capture.normalized();
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
            Judged::new(
                json!({"sweep": {"from": args.from, "to": args.to, "start_s": args.start_s, "duration_s": duration_s}, "response": r}),
                failures,
            )
        }
        Err(e) => Judged::new(
            json!({"sweep": {"from": args.from, "to": args.to}}),
            vec![format!("sweep: {e}")],
        ),
    }
}

fn judge_soak_capture(capture: &Capture, args: &SoakArgs) -> Judged {
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
            let observed = r.expected_failure_observed();
            Judged {
                report: json!({"soak": motion, "expected_failure_observed": observed, "result": r}),
                failures: r.failures.clone(),
                expected_failure_observed: observed,
            }
        }
        Err(e) => Judged::new(json!({"soak": motion}), vec![format!("soak: {e}")]),
    }
}

/// `--expect-fail`: the detectors' findings move to `detector_failures` as evidence, and the
/// verdict is whether the soak showed the step detector fired (and no dropout muddied it).
fn invert_for_expected_failure(
    report: &mut Value,
    detector_failures: Vec<String>,
    expected_failure_observed: bool,
) -> Vec<String> {
    report["detector_failures"] = json!(detector_failures);
    report["expect_failure"] = json!(true);
    if expected_failure_observed {
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
        let mut fired = json!({});
        assert!(invert_for_expected_failure(&mut fired, vec!["3 steps".into()], true).is_empty());
        assert_eq!(fired["detector_failures"][0], "3 steps");

        let mut quiet = json!({});
        assert_eq!(
            invert_for_expected_failure(&mut quiet, Vec::new(), false).len(),
            1
        );
        // A soak that could not be measured observes nothing, so a failure was not shown.
        let mut errored = json!({});
        assert_eq!(
            invert_for_expected_failure(&mut errored, vec!["x".into()], false).len(),
            1
        );
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("asperitas-measure").chain(args.iter().copied()))
    }

    #[test]
    fn clap_rejects_an_unknown_stimulus_and_the_exclusive_pairs() {
        assert!(parse(&["analyze", "c", "--stimulus", "square"]).is_err());
        assert!(parse(&["analyze", "c", "--stimulus", "pulse"]).is_ok());
        assert!(parse(&["analyze", "c", "--stimulus", "sine", "--excerpt", "x"]).is_err());
        assert!(parse(&["analyze", "c", "--excerpt", "x", "--expect-fail"]).is_err());
        assert!(parse(&["analyze"]).is_err());
        assert!(parse(&[]).is_err());
    }
}
