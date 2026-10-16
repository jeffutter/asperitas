//! Judge a knob sweep: as a parameter moves across its travel under host control
//! (`asperitas-rig override --sweep FROM:TO`), does the measured response move monotonically in the
//! intended direction without a flat region?
//!
//! The rig's `SET` schedule is linear in time (`control.rs` `schedule`), so the capture is divided
//! into [`thresholds::SWEEP_BUCKETS`] equal time spans, each standing for an equal slice of the
//! `from..to` knob travel. The capture does not record when the first `SET` landed, so the caller
//! gives `start_s` and `duration_s`, and each span is trimmed ([`thresholds::SWEEP_BUCKET_TRIM`])
//! to tolerate an error in them.
//!
//! The response per bucket, in dB:
//! - [`SweepKind::Gain`]: block RMS of the bucket.
//! - [`SweepKind::LowPass`]: RMS of the first difference, a fixed high-pass whose output follows
//!   the energy the low-pass is removing; it falls as the cutoff closes and rises as it opens.
//!
//! Buckets are ordered by ascending knob value, so a descending sweep is judged the same way. Both
//! kinds are expected to rise with the knob. Limits live in [`thresholds`].

use serde::Serialize;

use crate::thresholds;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepKind {
    Gain,
    LowPass,
}

impl SweepKind {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "gain" => Some(Self::Gain),
            "lowpass" => Some(Self::LowPass),
            _ => None,
        }
    }
}

/// Where the sweep sits in the capture. Knob levels are `0..=1`, as `asperitas-rig` takes them.
#[derive(Clone, Copy, Debug)]
pub struct SweepSpec {
    pub kind: SweepKind,
    pub from: f64,
    pub to: f64,
    /// Capture time of the first `SET`, seconds.
    pub start_s: f64,
    /// Length of the sweep from its first `SET`, seconds.
    pub duration_s: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Rising,
    Falling,
    /// The end-to-end change is under [`thresholds::SWEEP_MIN_SPAN_DB`].
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CurvePoint {
    /// Knob level at the bucket centre.
    pub value: f64,
    pub response_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FlatRegion {
    pub start_value: f64,
    pub end_value: f64,
    pub buckets: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SweepReport {
    pub kind: SweepKind,
    pub direction: Direction,
    pub expected_direction: Direction,
    /// End-to-end response change, last bucket minus first by ascending knob value, dB.
    pub span_db: f64,
    pub monotonic: bool,
    /// Largest fall against the expected direction from the best response so far, dB.
    pub max_reversal_db: f64,
    pub flat_regions: Vec<FlatRegion>,
    pub curve: Vec<CurvePoint>,
}

impl SweepReport {
    /// Reasons this sweep fails; empty when it passes.
    pub fn failures(&self) -> Vec<String> {
        let mut f = Vec::new();
        if self.direction != self.expected_direction {
            f.push(format!(
                "sweep response is {:?} (span {:.1} dB), expected {:?} with at least {} dB",
                self.direction,
                self.span_db,
                self.expected_direction,
                thresholds::SWEEP_MIN_SPAN_DB
            ));
        }
        if !self.monotonic {
            f.push(format!(
                "sweep response reverses by {:.1} dB (limit {} dB)",
                self.max_reversal_db,
                thresholds::SWEEP_MONOTONIC_TOL_DB
            ));
        }
        for r in &self.flat_regions {
            f.push(format!(
                "sweep response is flat from knob {:.2} to {:.2} ({} buckets)",
                r.start_value, r.end_value, r.buckets
            ));
        }
        f
    }
}

/// Why a sweep could not be measured; a property of the capture, so a failed verdict.
#[derive(Debug, PartialEq)]
pub struct SweepError(pub String);

impl std::fmt::Display for SweepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Measure and judge a sweep in a mono signal (full scale 1.0) at `rate_hz`.
pub fn judge_sweep(
    samples: &[f32],
    rate_hz: u32,
    spec: &SweepSpec,
) -> Result<SweepReport, SweepError> {
    let rate = f64::from(rate_hz);
    let n = thresholds::SWEEP_BUCKETS;
    if spec.duration_s.is_nan() || spec.duration_s <= 0.0 || spec.start_s < 0.0 {
        return Err(SweepError("sweep start/duration are not positive".into()));
    }
    let end = ((spec.start_s + spec.duration_s) * rate).round();
    if end > samples.len() as f64 + 0.5 {
        return Err(SweepError(format!(
            "capture holds {:.2} s but the sweep ends at {:.2} s",
            samples.len() as f64 / rate,
            spec.start_s + spec.duration_s
        )));
    }
    let bucket_s = spec.duration_s / n as f64;
    let floor = f64::from(thresholds::PARAM_STEP_FLOOR_RMS);

    let mut curve = Vec::with_capacity(n);
    for k in 0..n {
        let t0 = spec.start_s + bucket_s * (k as f64 + thresholds::SWEEP_BUCKET_TRIM);
        let t1 = spec.start_s + bucket_s * (k as f64 + 1.0 - thresholds::SWEEP_BUCKET_TRIM);
        let (a, b) = ((t0 * rate) as usize, (t1 * rate) as usize);
        if b.saturating_sub(a) < thresholds::SWEEP_MIN_WINDOW_SAMPLES {
            return Err(SweepError(format!(
                "sweep too short: a bucket window is {} samples, need {}",
                b.saturating_sub(a),
                thresholds::SWEEP_MIN_WINDOW_SAMPLES
            )));
        }
        let w = &samples[a..b];
        let ms = match spec.kind {
            SweepKind::Gain => {
                w.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / w.len() as f64
            }
            SweepKind::LowPass => {
                w.windows(2)
                    .map(|p| f64::from(p[1] - p[0]).powi(2))
                    .sum::<f64>()
                    / (w.len() - 1) as f64
            }
        };
        let frac = (k as f64 + 0.5) / n as f64;
        curve.push(CurvePoint {
            value: spec.from + (spec.to - spec.from) * frac,
            response_db: 20.0 * ms.sqrt().max(floor).log10(),
        });
    }
    if spec.to < spec.from {
        curve.reverse();
    }

    let span_db = curve[n - 1].response_db - curve[0].response_db;
    let direction = if span_db.abs() < thresholds::SWEEP_MIN_SPAN_DB {
        Direction::None
    } else if span_db > 0.0 {
        Direction::Rising
    } else {
        Direction::Falling
    };

    // Fall from the best response so far; judged against the expected (rising) direction.
    let mut best = f64::NEG_INFINITY;
    let mut max_reversal_db = 0.0_f64;
    for p in &curve {
        best = best.max(p.response_db);
        max_reversal_db = max_reversal_db.max(best - p.response_db);
    }

    let mut flat_regions = Vec::new();
    let mut run_start = 0;
    for i in 1..=n {
        let flat = i < n
            && (curve[i].response_db - curve[i - 1].response_db).abs()
                < thresholds::SWEEP_FLAT_EPS_DB;
        if flat {
            continue;
        }
        let steps = i - 1 - run_start;
        if steps >= thresholds::SWEEP_FLAT_MIN_STEPS {
            flat_regions.push(FlatRegion {
                start_value: curve[run_start].value,
                end_value: curve[i - 1].value,
                buckets: steps + 1,
            });
        }
        run_start = i;
    }

    Ok(SweepReport {
        kind: spec.kind,
        direction,
        expected_direction: Direction::Rising,
        span_db,
        monotonic: max_reversal_db <= thresholds::SWEEP_MONOTONIC_TOL_DB,
        max_reversal_db,
        flat_regions,
        curve,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: u32 = 48_000;
    const DUR: f64 = 10.0;

    fn spec(kind: SweepKind, from: f64, to: f64) -> SweepSpec {
        SweepSpec {
            kind,
            from,
            to,
            start_s: 0.0,
            duration_s: DUR,
        }
    }

    /// Per-bucket gain in dB as a function of knob fraction `0..1`, applied to a 1 kHz sine.
    fn gain_signal(db_at: impl Fn(f64) -> f64, descending: bool) -> Vec<f32> {
        let total = (DUR * f64::from(RATE)) as usize;
        (0..total)
            .map(|i| {
                let t = i as f64 / total as f64;
                let knob = if descending { 1.0 - t } else { t };
                let amp = 0.5 * 10f64.powf(db_at(knob) / 20.0);
                (amp * f64::from((TAU * 1000.0 * i as f32 / RATE as f32).sin())) as f32
            })
            .collect()
    }

    /// A white-ish noise whose high band follows `db_at`, for the low-pass response.
    fn lowpass_signal(db_at: impl Fn(f64) -> f64) -> Vec<f32> {
        let total = (DUR * f64::from(RATE)) as usize;
        let mut state = 12345u32;
        (0..total)
            .map(|i| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (state >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0;
                let t = i as f64 / total as f64;
                (0.1 * 10f64.powf(db_at(t) / 20.0)) as f32 * noise
            })
            .collect()
    }

    /// Healthy: 24 dB over the travel, from -30 dB.
    fn ramp(k: f64) -> f64 {
        -30.0 + 24.0 * k
    }

    fn judged(kind: SweepKind, s: &[f32]) -> SweepReport {
        judge_sweep(s, RATE, &spec(kind, 0.0, 1.0)).unwrap()
    }

    #[test]
    fn monotonic_gain_and_lowpass_pass() {
        let g = judged(SweepKind::Gain, &gain_signal(ramp, false));
        assert_eq!(g.failures(), Vec::<String>::new(), "{g:?}");
        assert_eq!(g.direction, Direction::Rising);
        assert!((g.span_db - 24.0 * 0.95).abs() < 1.0, "{}", g.span_db);
        let l = judged(SweepKind::LowPass, &lowpass_signal(ramp));
        assert_eq!(l.failures(), Vec::<String>::new(), "{l:?}");
    }

    #[test]
    fn descending_sweep_is_judged_by_knob_value() {
        // Knob goes 1 -> 0 while time advances, and the sound falls with it.
        let s = gain_signal(ramp, true);
        let r = judge_sweep(&s, RATE, &spec(SweepKind::Gain, 1.0, 0.0)).unwrap();
        assert_eq!(r.failures(), Vec::<String>::new(), "{r:?}");
        assert!(r.curve[0].value < r.curve[19].value);
    }

    #[test]
    fn plateau_fails_as_flat_region() {
        // Level held at its 0.4 value from 0.4 to 0.7: a plateau mid-travel.
        let r = judged(
            SweepKind::Gain,
            &gain_signal(
                |k| ramp(if (0.4..0.7).contains(&k) { 0.4 } else { k }),
                false,
            ),
        );
        assert!(!r.flat_regions.is_empty(), "{r:?}");
        assert!(r.failures().iter().any(|f| f.contains("flat")), "{r:?}");
    }

    #[test]
    fn reversal_fails_as_non_monotonic() {
        // Dips 6 dB over the middle of the travel, then recovers.
        let r = judged(
            SweepKind::Gain,
            &gain_signal(
                |k| ramp(k) - if (0.45..0.65).contains(&k) { 6.0 } else { 0.0 },
                false,
            ),
        );
        assert!(!r.monotonic, "{r:?}");
        assert!(r.max_reversal_db > thresholds::SWEEP_MONOTONIC_TOL_DB);
        assert!(r.failures().iter().any(|f| f.contains("reverses")), "{r:?}");
    }

    #[test]
    fn dead_zone_fails() {
        // Silence over the first third of the travel: the knob does nothing audible there.
        let r = judged(
            SweepKind::Gain,
            &gain_signal(|k| if k < 0.35 { -200.0 } else { ramp(k) }, false),
        );
        assert!(!r.flat_regions.is_empty(), "{r:?}");
        assert!(!r.failures().is_empty());
        let l = judged(
            SweepKind::LowPass,
            &lowpass_signal(|k| if k < 0.35 { -200.0 } else { ramp(k) }),
        );
        assert!(!l.flat_regions.is_empty(), "{l:?}");
    }

    #[test]
    fn wrong_direction_and_no_response_fail() {
        let falling = judged(SweepKind::Gain, &gain_signal(|k| ramp(1.0 - k), false));
        assert_eq!(falling.direction, Direction::Falling);
        assert!(falling.failures().iter().any(|f| f.contains("expected")));
        let none = judged(SweepKind::Gain, &gain_signal(|_| -10.0, false));
        assert_eq!(none.direction, Direction::None);
        assert!(!none.failures().is_empty());
    }

    #[test]
    fn ripple_inside_tolerance_passes() {
        let r = judged(
            SweepKind::Gain,
            &gain_signal(|k| ramp(k) + 0.4 * (k * 80.0).sin(), false),
        );
        assert_eq!(r.failures(), Vec::<String>::new(), "{r:?}");
    }

    #[test]
    fn too_short_or_misplaced_sweep_is_an_error() {
        let s = gain_signal(ramp, false);
        let mut sp = spec(SweepKind::Gain, 0.0, 1.0);
        sp.start_s = 2.0;
        assert!(judge_sweep(&s, RATE, &sp).is_err());
        sp.start_s = 0.0;
        sp.duration_s = 0.05;
        assert!(judge_sweep(&s, RATE, &sp).is_err());
    }
}
