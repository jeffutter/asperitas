//! Judge a steady-stimulus loopback capture (no session sidecar) against the rig baseline.
//!
//! Only the sine is judged against the limits in [`crate::thresholds`] (level, silence,
//! discontinuities, spurs, parameter steps). The sweep and pulse are reported and checked for
//! alignment stability only, because the baseline limits are defined for the sine.

use asperitas_stimulus::stimulus::{ExponentialSweepParams, PulseTrainParams, SineParams};
use clap::ValueEnum;
use serde::Serialize;

use crate::align::{align, assert_stable, Alignment};
use crate::capture::Capture;
use crate::metrics::{analyze, Metrics};
use crate::param_step::{detect_param_steps, ParamSteps};
use crate::stimulus::Stimulus;
use crate::thresholds;

/// Which rig generator the capture holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum StimulusKind {
    Sine,
    Sweep,
    Pulse,
}

impl StimulusKind {
    fn stimulus(self) -> Stimulus {
        match self {
            StimulusKind::Sine => Stimulus::Sine(SineParams::default()),
            StimulusKind::Sweep => Stimulus::Sweep(ExponentialSweepParams::default()),
            StimulusKind::Pulse => Stimulus::Pulse(PulseTrainParams::default()),
        }
    }

    /// Whether the baseline limits are defined for this stimulus.
    fn judged_against_baseline(self) -> bool {
        self == StimulusKind::Sine
    }
}

/// Alignment summary: how many windows and the median loopback delay.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AlignmentSummary {
    pub windows: usize,
    pub median_delay_samples: f64,
}

impl From<&Alignment> for AlignmentSummary {
    fn from(a: &Alignment) -> Self {
        AlignmentSummary {
            windows: a.delays.len(),
            median_delay_samples: a.median(),
        }
    }
}

/// The measurements and verdict for one steady-stimulus capture. A capture too short to align
/// carries only `stimulus` and its failure.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BaselineReport {
    pub stimulus: StimulusKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judged_against_baseline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<AlignmentSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param_steps: Option<ParamSteps>,
    /// Reasons the capture fails; empty means it passes.
    #[serde(skip)]
    pub failures: Vec<String>,
}

/// Measure `capture` against the regenerated `kind` stimulus and judge it where the baseline
/// applies.
pub fn judge_baseline(capture: &Capture, kind: StimulusKind) -> BaselineReport {
    let rendered = kind.stimulus().render(capture.samples.len());
    let mut failures = Vec::new();

    let alignment = match align(&capture.samples, &rendered) {
        Ok(a) => a,
        Err(e) => {
            // Too short to align is a property of the capture, so a verdict, not a usage error.
            failures.push(format!("alignment: {e}"));
            return BaselineReport {
                stimulus: kind,
                judged_against_baseline: None,
                alignment: None,
                metrics: None,
                param_steps: None,
                failures,
            };
        }
    };
    if let Err(e) = assert_stable(&alignment) {
        failures.push(format!("alignment: {e}"));
    }
    let metrics = analyze(capture, &rendered, &alignment);
    let steps = detect_param_steps(&capture.normalized());

    let judged = kind.judged_against_baseline();
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

    BaselineReport {
        stimulus: kind,
        judged_against_baseline: Some(judged),
        alignment: Some(AlignmentSummary::from(&alignment)),
        metrics: Some(metrics),
        param_steps: Some(steps),
        failures,
    }
}
