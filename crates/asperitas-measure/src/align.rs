//! Align a capture to the stimulus that produced it, window by window.
//!
//! Each window of the capture is cross-correlated (FFT) against the stimulus span that could have
//! produced it, giving a delay in samples: integer peak plus parabolic subsample refinement. The
//! rig plays and captures on one clock domain, so the delay must be the same in every window and
//! in every repeat capture. A change is a slip and is reported as [`AlignError::Slip`]; it is never
//! absorbed as drift.
//!
//! Delay convention: `capture[n] ~= stimulus[n - delay]`, so a positive delay is latency.
//!
//! Periodic stimuli (the sine) look identical one period later, so for them the delay is only
//! determined modulo the period. The search range is bounded by [`thresholds::ALIGN_MAX_DELAY_SAMPLES`]
//! and a slip of exactly one whole period is invisible to any correlation method.

use std::fmt;

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use crate::thresholds;

/// Per-window delays of a capture against its stimulus, in samples.
#[derive(Clone, Debug, PartialEq)]
pub struct Alignment {
    /// One delay per full window, in capture order.
    pub delays: Vec<f64>,
    pub window_samples: usize,
}

impl Alignment {
    /// Median window delay, robust to a single bad window.
    pub fn median(&self) -> f64 {
        let mut v = self.delays.clone();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AlignError {
    /// The capture is shorter than one window.
    TooShort { have: usize, need: usize },
    /// The stimulus is shorter than the capture span being aligned.
    StimulusTooShort { have: usize, need: usize },
    /// A window's delay differs from the first window's by more than the tolerance.
    Slip {
        window: usize,
        expected: f64,
        got: f64,
    },
    /// Two captures disagree on delay.
    RepeatMismatch { first: f64, second: f64 },
}

impl fmt::Display for AlignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlignError::TooShort { have, need } => {
                write!(f, "capture has {have} samples, need at least {need}")
            }
            AlignError::StimulusTooShort { have, need } => {
                write!(f, "stimulus has {have} samples, need {need}")
            }
            AlignError::Slip {
                window,
                expected,
                got,
            } => write!(
                f,
                "alignment slip at window {window}: delay {got:.3} vs {expected:.3} samples"
            ),
            AlignError::RepeatMismatch { first, second } => write!(
                f,
                "repeat captures disagree: delay {first:.3} vs {second:.3} samples"
            ),
        }
    }
}

impl std::error::Error for AlignError {}

/// Delay of each full window of `samples` against `stimulus` (rendered from sample 0 of the
/// capture's start; at least as long as the span being aligned).
pub fn align(samples: &[i16], stimulus: &[f32]) -> Result<Alignment, AlignError> {
    let w = thresholds::ALIGN_WINDOW_SAMPLES;
    let max_d = thresholds::ALIGN_MAX_DELAY_SAMPLES;
    let n_win = samples.len() / w;
    if n_win == 0 {
        return Err(AlignError::TooShort {
            have: samples.len(),
            need: w,
        });
    }
    if stimulus.len() < n_win * w {
        return Err(AlignError::StimulusTooShort {
            have: stimulus.len(),
            need: n_win * w,
        });
    }

    // Reference span for window k is stimulus[k*w - max_d .. k*w + w], zero before the start.
    let ref_len = w + max_d;
    let fft_len = (ref_len + w).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(fft_len);
    let inv = planner.plan_fft_inverse(fft_len);

    let mut anchor: Option<f64> = None;
    let mut delays = Vec::with_capacity(n_win);
    for k in 0..n_win {
        let start = k * w;
        let mut c = vec![Complex::new(0.0f32, 0.0); fft_len];
        let mut s = vec![Complex::new(0.0f32, 0.0); fft_len];
        for (dst, &v) in c.iter_mut().zip(&samples[start..start + w]) {
            dst.re = asperitas_dsp::pcm::i16_to_f32(v);
        }
        for (j, dst) in s.iter_mut().take(ref_len).enumerate() {
            if let Some(&v) = (start + j)
                .checked_sub(max_d)
                .and_then(|idx| stimulus.get(idx))
            {
                dst.re = v;
            }
        }
        fwd.process(&mut c);
        fwd.process(&mut s);
        // R[m] = sum_i c[i] * s[i + m]  =>  IFFT(conj(C) * S)
        for (cv, sv) in c.iter_mut().zip(&s) {
            *cv = cv.conj() * *sv;
        }
        inv.process(&mut c);
        // m = max_d - delay, valid for delay in 0..=max_d.
        let r: Vec<f32> = (0..=max_d).map(|m| c[m].re).collect();
        // Search range as indices m = max_d - delay. The first window searches everything and
        // anchors; later windows track within +-ALIGN_TRACK_SAMPLES of the anchor so that a
        // periodic stimulus cannot jump to a neighbouring period.
        let (lo, hi) = match anchor {
            None => (0, max_d),
            Some(d) => {
                let m = max_d as f64 - d;
                let t = thresholds::ALIGN_TRACK_SAMPLES as f64;
                (
                    (m - t).max(0.0) as usize,
                    ((m + t).ceil() as usize).min(max_d),
                )
            }
        };
        let peak = (lo..=hi).map(|m| r[m]).fold(f32::MIN, f32::max);
        let best = if anchor.is_none() {
            // Equal peaks one period apart: take the smallest delay within the tie fraction.
            let floor = peak * thresholds::ALIGN_TIE_FRACTION;
            (lo..=hi)
                .rev()
                .find(|&m| r[m] >= floor && is_local_max(&r, m))
                .unwrap_or(lo)
        } else {
            (lo..=hi).find(|&m| r[m] == peak).unwrap_or(lo)
        };
        let frac = if best > 0 && best < max_d {
            let (a, b, cc) = (r[best - 1], r[best], r[best + 1]);
            let den = a - 2.0 * b + cc;
            if den.abs() > f32::EPSILON {
                f64::from(0.5 * (a - cc) / den)
            } else {
                0.0
            }
        } else {
            0.0
        };
        let delay = max_d as f64 - (best as f64 + frac);
        anchor.get_or_insert(delay);
        delays.push(delay);
    }
    Ok(Alignment {
        delays,
        window_samples: w,
    })
}

fn is_local_max(r: &[f32], m: usize) -> bool {
    (m == 0 || r[m] >= r[m - 1]) && (m + 1 >= r.len() || r[m] >= r[m + 1])
}

/// Every window's delay must match the first window's within the stability tolerance.
pub fn assert_stable(a: &Alignment) -> Result<(), AlignError> {
    let expected = a.delays[0];
    for (window, &got) in a.delays.iter().enumerate() {
        if (got - expected).abs() > thresholds::ALIGN_STABLE_TOL_SAMPLES {
            return Err(AlignError::Slip {
                window,
                expected,
                got,
            });
        }
    }
    Ok(())
}

/// Two captures of the same stimulus must have the same delay within the repeat tolerance.
#[cfg(test)]
pub fn assert_repeatable(a: &Alignment, b: &Alignment) -> Result<(), AlignError> {
    let (first, second) = (a.median(), b.median());
    if (first - second).abs() > thresholds::ALIGN_STABLE_TOL_SAMPLES {
        return Err(AlignError::RepeatMismatch { first, second });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::Capture;
    use crate::stimulus::Stimulus;
    use asperitas_stimulus::stimulus::SineParams;

    fn load(name: &str) -> Capture {
        Capture::from_console_zst(format!(
            "{}/../../captures/{name}.console.zst",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn sine(n: usize) -> Vec<f32> {
        Stimulus::Sine(SineParams::default()).render(n)
    }

    /// A capture that is `stim` delayed by `delay` whole samples, scaled and quantised.
    fn delayed(stim: &[f32], delay: usize, n: usize) -> Capture {
        let samples = (0..n)
            .map(|i| i.checked_sub(delay).map_or(0.0, |j| stim[j]))
            .map(|v| (v * 0.9 * crate::capture::FULL_SCALE as f32) as i16)
            .collect();
        Capture {
            samples,
            sample_rate_hz: 48_000,
            blocks: 1,
        }
    }

    #[test]
    fn fixture_300s_sine_is_stable() {
        let c = load("TASK-038.05/2026-10-07-rig-sine-300s");
        let a = align(&c.samples, &sine(c.samples.len())).unwrap();
        assert_eq!(a.delays.len(), 300);
        assert_stable(&a).unwrap();
    }

    #[test]
    fn fixture_reset_pair_repeats() {
        let (c1, c2) = (
            load("TASK-059.04/2026-10-13-rig-10s-reset1"),
            load("TASK-059.04/2026-10-13-rig-10s-reset2"),
        );
        let a1 = align(&c1.samples, &sine(c1.samples.len())).unwrap();
        let a2 = align(&c2.samples, &sine(c2.samples.len())).unwrap();
        assert_stable(&a1).unwrap();
        assert_stable(&a2).unwrap();
        assert_repeatable(&a1, &a2).unwrap();
    }

    #[test]
    fn synthetic_delay_is_recovered() {
        let n = 5 * 48_000;
        let st = sine(n);
        let a = align(&delayed(&st, 37, n).samples, &st).unwrap();
        assert_stable(&a).unwrap();
        assert!((a.median() - 37.0).abs() < 0.05, "median {}", a.median());
    }

    #[test]
    fn synthetic_one_sample_slip_is_detected() {
        let n = 5 * 48_000;
        let st = sine(n);
        let mut c = delayed(&st, 37, n);
        // Drop one sample mid-signal: everything after arrives one sample earlier.
        c.samples.remove(n / 2);
        c.samples.push(0);
        let a = align(&c.samples, &st).unwrap();
        match assert_stable(&a) {
            Err(AlignError::Slip { window, .. }) => {
                assert!((2..=3).contains(&window), "window {window}")
            }
            other => panic!("expected slip, got {other:?}"),
        }
    }

    #[test]
    fn repeat_with_different_delay_is_rejected() {
        let n = 3 * 48_000;
        let st = sine(n);
        let a = align(&delayed(&st, 37, n).samples, &st).unwrap();
        let b = align(&delayed(&st, 38, n).samples, &st).unwrap();
        assert!(matches!(
            assert_repeatable(&a, &b),
            Err(AlignError::RepeatMismatch { .. })
        ));
    }

    #[test]
    fn short_inputs_are_typed_errors() {
        let st = sine(100);
        let c = delayed(&st, 0, 100);
        assert!(matches!(
            align(&c.samples, &st),
            Err(AlignError::TooShort { .. })
        ));
    }
}
