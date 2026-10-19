//! Judge a long capture taken while a parameter moves (`asperitas-rig session` with a soak or hold): one
//! verdict over silence dropouts, discontinuity candidates and parameter steps for the whole run,
//! with each candidate attributed to the parameter-motion window or to the steady time outside it.
//!
//! The verdict does not depend on attribution. Any silence run, discontinuity candidate or
//! parameter step anywhere fails the soak, because the criterion is "none above threshold over the
//! whole run". The inside/outside split is evidence for the person reading a failure: a candidate
//! inside the window points at the parameter path (smoothing, mapping), one outside at the
//! hardware or the host.
//!
//! The discontinuity detector refits the loop gain every
//! `thresholds::SOAK_GAIN_FIT_BLOCK_SAMPLES` samples (see `GainFit::Tracking`), because a
//! swept gain makes the fixed-gain model of [`crate::metrics`] drift away from the capture. A
//! sweep toward silence legitimately produces silence runs, so sweep ranges must stay above the
//! silence floor ([`thresholds::SILENCE_LSB`]). Positions lists are capped by their detectors, but
//! the counts are exact; candidates beyond the cap are reported as `unattributed`.
//!
//! `expect_failure` asserts sensitivity: an unsmoothed build is supposed to fail, and it counts as
//! evidence only when the step detector specifically fired, so a dropout cannot pass for proof
//! that the step detector works (its edges read as level steps too, so silence disqualifies the
//! evidence).

use serde::Serialize;

use crate::align::{align, assert_stable};
use crate::capture::Capture;
use crate::metrics::{discontinuities, silence, GainFit};
use crate::param_step::detect_param_steps;
use crate::stimulus::Stimulus;
use crate::thresholds;

/// Where the parameter motion sits in the capture; the schedule is linear in time, as the rig's.
#[derive(Clone, Copy, Debug)]
pub struct SoakSpec {
    /// Knob levels, `0..=1`; equal means the parameter is held (no motion window).
    pub from: f64,
    pub to: f64,
    /// Capture time of the first `SET`, seconds.
    pub start_s: f64,
    /// Length of the motion from its first `SET`, seconds.
    pub duration_s: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MotionWindow {
    pub start_s: f64,
    pub end_s: f64,
}

/// One detector's candidates, split by where they fall.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Attribution {
    /// Exact count from the detector.
    pub count: usize,
    pub inside_motion: usize,
    pub outside_motion: usize,
    /// Candidates the detector counted but did not list a position for.
    pub unattributed: usize,
    /// Capture time of each listed candidate, seconds.
    pub times_s: Vec<f64>,
}

impl Attribution {
    fn new(count: usize, positions: &[usize], rate: f64, window: Option<&MotionWindow>) -> Self {
        let times_s: Vec<f64> = positions.iter().map(|&p| p as f64 / rate).collect();
        let inside_motion = window.map_or(0, |w| {
            times_s
                .iter()
                .filter(|&&t| t >= w.start_s && t <= w.end_s)
                .count()
        });
        Self {
            count,
            inside_motion,
            outside_motion: positions.len() - inside_motion,
            unattributed: count.saturating_sub(positions.len()),
            times_s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SoakReport {
    pub duration_s: f64,
    pub motion_window: Option<MotionWindow>,
    pub silence: Attribution,
    pub discontinuities: Attribution,
    pub max_discontinuity_jump_lsb: f64,
    pub param_steps: Attribution,
    pub max_param_step_db: f64,
    /// Reasons the capture fails, whether or not a failure was expected.
    pub failures: Vec<String>,
}

impl SoakReport {
    /// The step detector fired and no dropout muddies it: a dropout's edges also read as level steps,
    /// so a capture with silence cannot show the unsmoothed path is what the detector caught.
    pub fn expected_failure_observed(&self) -> bool {
        self.param_steps.count > 0 && self.silence.count == 0
    }
}

/// Why a soak could not be measured; a property of the capture, so a failed verdict.
#[derive(Debug, PartialEq)]
pub struct SoakError(pub String);

impl std::fmt::Display for SoakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Analyze a soak capture against the rig stimulus it was recorded with.
pub fn judge_soak(
    capture: &Capture,
    stimulus: &Stimulus,
    spec: &SoakSpec,
) -> Result<SoakReport, SoakError> {
    let rate = f64::from(capture.sample_rate_hz);
    let duration_s = capture.samples.len() as f64 / rate;
    if spec.duration_s.is_nan() || spec.duration_s < 0.0 || spec.start_s < 0.0 {
        return Err(SoakError("soak start/duration are negative".into()));
    }
    let mut failures = Vec::new();
    if duration_s < thresholds::SOAK_MIN_SECONDS {
        failures.push(format!(
            "capture is {duration_s:.1} s, a soak needs at least {} s",
            thresholds::SOAK_MIN_SECONDS
        ));
    }

    let moving = (spec.to - spec.from).abs() > f64::EPSILON && spec.duration_s > 0.0;
    let window = moving.then(|| MotionWindow {
        start_s: (spec.start_s - thresholds::SOAK_MOTION_MARGIN_S).max(0.0),
        end_s: spec.start_s + spec.duration_s + thresholds::SOAK_MOTION_MARGIN_S,
    });

    let rendered = stimulus.render(capture.samples.len());
    let alignment =
        align(&capture.samples, &rendered).map_err(|e| SoakError(format!("alignment: {e}")))?;
    if let Err(e) = assert_stable(&alignment) {
        failures.push(format!("alignment: {e}"));
    }

    let x: Vec<f64> = capture.samples.iter().map(|&s| f64::from(s)).collect();
    let silent = silence(&capture.samples);
    let disc = discontinuities(
        &x,
        &rendered,
        &alignment,
        GainFit::Tracking {
            block: thresholds::SOAK_GAIN_FIT_BLOCK_SAMPLES,
        },
    );
    let steps = detect_param_steps(&capture.normalized());

    let silence_a = Attribution::new(silent.run_count, &silent.run_starts, rate, window.as_ref());
    let disc_a = Attribution::new(disc.count, &disc.positions, rate, window.as_ref());
    let steps_a = Attribution::new(steps.count, &steps.positions, rate, window.as_ref());
    for (name, a) in [
        ("silent run(s)", &silence_a),
        ("discontinuity candidate(s)", &disc_a),
        ("parameter step(s)", &steps_a),
    ] {
        if a.count > 0 {
            failures.push(format!(
                "{} {name}: {} inside parameter motion, {} outside, {} unattributed",
                a.count, a.inside_motion, a.outside_motion, a.unattributed
            ));
        }
    }

    Ok(SoakReport {
        duration_s,
        motion_window: window,
        silence: silence_a,
        discontinuities: disc_a,
        max_discontinuity_jump_lsb: disc.max_jump_lsb,
        param_steps: steps_a,
        max_param_step_db: steps.max_step_db,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::FULL_SCALE;
    use asperitas_dsp::gain::{Gain, GainParams};
    use asperitas_dsp::processor::Processor;
    use asperitas_stimulus::stimulus::SineParams;

    const RATE: usize = 48_000;
    const SECONDS: usize = 310;
    /// The rig sends 20 `SET`/s.
    const SET_EVERY: usize = RATE / 20;
    const SWEEP_S: f64 = 300.0;
    const HEAD: usize = 100;

    fn sine() -> Stimulus {
        Stimulus::Sine(SineParams::default())
    }

    /// The rig sine through `Gain` whose `gain_db` follows `db_at(seconds)`, set every
    /// `SET_EVERY` samples; `snap` skips the smoothing on each change. A 0.4 dB loss stands in for
    /// the cable, and the capture starts 100 samples into the signal (an offset the 1 kHz sine
    /// cannot tell from a delay of 4 samples), so it has no silent head.
    fn capture(db_at: impl Fn(f32) -> f32, snap: bool) -> Capture {
        let n = RATE * SECONDS;
        let x = sine().render(n + HEAD);
        let mut gain = Gain::default();
        gain.set_sample_rate(RATE as f32);
        gain.set_params(&GainParams {
            gain_db: db_at(0.0_f32),
        });
        gain.reset();
        let mut last = db_at(0.0);
        let loop_gain = 10f32.powf(-0.41 / 20.0);
        let mut samples = vec![0i16; n];
        for i in 0..n + HEAD {
            if i % SET_EVERY == 0 {
                let db = db_at(i as f32 / RATE as f32);
                if db != last {
                    gain.set_params(&GainParams { gain_db: db });
                    if snap {
                        gain.reset();
                    }
                    last = db;
                }
            }
            let y = gain.tick([x[i], x[i]])[0] * loop_gain;
            if i >= HEAD {
                samples[i - HEAD] = (y * FULL_SCALE as f32).round() as i16;
            }
        }
        Capture {
            samples,
            sample_rate_hz: RATE as u32,
            blocks: 0,
        }
    }

    /// Continuous ramp from -24 dB to +6 dB over the sweep.
    fn ramp(t: f32) -> f32 {
        -24.0 + 30.0 * (t / SWEEP_S as f32).min(1.0)
    }

    /// Five 6 dB plateaus: coarse enough that snapping them is audible.
    fn coarse(t: f32) -> f32 {
        -24.0 + 6.0 * (t / 60.0).floor().min(4.0)
    }

    fn spec() -> SoakSpec {
        SoakSpec {
            from: 0.0,
            to: 1.0,
            start_s: 0.0,
            duration_s: SWEEP_S,
        }
    }

    #[test]
    fn smoothed_ramp_passes() {
        let r = judge_soak(&capture(ramp, false), &sine(), &spec()).unwrap();
        assert!(r.failures.is_empty(), "{r:#?}");
        assert!(!r.expected_failure_observed());
    }

    #[test]
    fn unsmoothed_steps_fail_inside_the_motion_window() {
        let r = judge_soak(&capture(coarse, true), &sine(), &spec()).unwrap();
        assert!(!r.failures.is_empty());
        assert!(r.param_steps.count >= 4, "{r:#?}");
        assert_eq!(r.param_steps.outside_motion, 0, "{r:#?}");
        assert!(r.param_steps.inside_motion >= 4);
        assert!(r.expected_failure_observed());
    }

    #[test]
    fn smoothed_coarse_steps_do_not_trip_the_step_detector() {
        // The sensitivity pair of the test above: same schedule, smoothing on.
        let r = judge_soak(&capture(coarse, false), &sine(), &spec()).unwrap();
        assert_eq!(r.param_steps.count, 0, "{r:#?}");
    }

    /// The rig's sine path: `FxChain<Sine>` with the patch selected, the gain knob following `coarse` at 20
    /// `SET`/s through the live override, exactly as the firmware callback drives it.
    fn chain_capture(smoothed: bool) -> Capture {
        use asperitas_stimulus::fx_chain::FxChain;
        use asperitas_stimulus::stimulus::Sine;
        let n = RATE * SECONDS;
        let mut sine = Sine::default();
        sine.set_sample_rate(RATE as f32);
        let mut chain = if smoothed {
            FxChain::new(sine)
        } else {
            FxChain::new_unsmoothed(sine)
        };
        chain.select(1, 0);
        // Settled at the schedule's starting level before the capture, so the first captured
        // block is not itself a step or a ramp.
        let start = (coarse(0.0) + 60.0) / 72.0;
        for _ in 0..2000 {
            chain.process_block_with(&mut [[0.0f32; 2]; 32], |base| [base[0], start]);
        }
        chain.reset();
        let mut samples = Vec::with_capacity(n);
        let mut knob = 0.0f32;
        let mut block = 0usize;
        while samples.len() < n {
            // 20 SET/s, quantised to whole 32-sample callbacks as the rig's are.
            if (block * 32) % SET_EVERY < 32 {
                let t = (block * 32) as f32 / RATE as f32;
                // Map coarse()'s dB plateaus back to the knob: gain_db = -60 + 72 * knob.
                knob = (coarse(t) + 60.0) / 72.0;
            }
            let mut out = [[0.0f32; 2]; 32];
            chain.process_block_with(&mut out, |base| [base[0], knob]);
            samples.extend(
                out.iter()
                    .map(|f| (f[0] * FULL_SCALE as f32).round() as i16),
            );
            block += 1;
        }
        samples.truncate(n);
        Capture {
            samples,
            sample_rate_hz: RATE as u32,
            blocks: 0,
        }
    }

    #[test]
    fn rig_sine_chain_unsmoothed_trips_the_step_detector_and_smoothed_does_not() {
        let step = judge_soak(&chain_capture(false), &sine(), &spec()).unwrap();
        assert!(step.param_steps.count >= 4, "{step:#?}");
        assert!(step.expected_failure_observed(), "{step:#?}");
        let smooth = judge_soak(&chain_capture(true), &sine(), &spec()).unwrap();
        assert_eq!(smooth.param_steps.count, 0, "{smooth:#?}");
    }

    #[test]
    fn dropout_outside_the_motion_fails_as_silence_outside() {
        let mut c = capture(ramp, false);
        let at = 305 * RATE;
        c.samples[at..at + 2000].fill(0);
        let r = judge_soak(&c, &sine(), &spec()).unwrap();
        assert!(!r.failures.is_empty());
        assert_eq!(r.silence.count, 1, "{r:#?}");
        assert_eq!((r.silence.inside_motion, r.silence.outside_motion), (0, 1));
        assert!(r.discontinuities.count >= 1, "{r:#?}");
        assert!(!r.expected_failure_observed(), "a dropout is not a step");
    }

    #[test]
    fn dropout_inside_the_motion_is_attributed_inside() {
        let mut c = capture(ramp, false);
        let at = 150 * RATE;
        c.samples[at..at + 2000].fill(0);
        let r = judge_soak(&c, &sine(), &spec()).unwrap();
        assert_eq!((r.silence.inside_motion, r.silence.outside_motion), (1, 0));
    }

    #[test]
    fn short_capture_fails() {
        let mut c = capture(ramp, false);
        c.samples.truncate(RATE * 120);
        let r = judge_soak(&c, &sine(), &spec()).unwrap();
        assert!(r.failures.iter().any(|f| f.contains("at least")), "{r:#?}");
    }
}
