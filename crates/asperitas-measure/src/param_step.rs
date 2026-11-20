//! Detector for audible steps caused by parameter changes.
//!
//! A parameter change that is not smoothed shows up as an abrupt change in signal level. The
//! detector walks the signal in hops of [`thresholds::PARAM_STEP_BLOCK_SAMPLES`] and, at each hop,
//! compares the RMS of the [`thresholds::PARAM_STEP_WINDOW_SAMPLES`] before it with the RMS of the
//! same length after it, flagging a change over [`thresholds::PARAM_STEP_MAX_DB`]. The window spans
//! about a cycle of the lowest expected pitch, so the reading does not depend on a tone's period.
//! Flags within [`thresholds::PARAM_STEP_MERGE_BLOCKS`] hops merge into one event, positioned at
//! its largest change, where the windows straddle the step evenly. Windows below the noise floor
//! are never compared.
//!
//! The tests prove the detector can fire: the same gain change flags when snapped and passes
//! through the smoothed `Gain` path.

use serde::{Deserialize, Serialize};

use crate::thresholds;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamSteps {
    /// Exact number of merged events.
    pub count: usize,
    /// Sample index of the largest level change of each event, capped at `PARAM_STEP_MAX_POSITIONS`.
    pub positions: Vec<usize>,
    /// Largest before/after window level change seen, dB (0 when no comparable blocks).
    pub max_step_db: f64,
}

/// Find parameter-step events in a mono signal (full scale 1.0).
pub fn detect_param_steps(samples: &[f32]) -> ParamSteps {
    detect_param_steps_windowed(samples, thresholds::PARAM_STEP_WINDOW_SAMPLES)
}

/// [`detect_param_steps`] with an explicit comparison window, samples. A longer window rides over
/// the amplitude beating of ringing, multi-partial material (which a short window reads as steps)
/// at the cost of seeing more of a smoothed ramp, so a caller listening to music picks
/// [`thresholds::PARAM_STEP_MUSIC_WINDOW_SAMPLES`].
pub fn detect_param_steps_windowed(samples: &[f32], win: usize) -> ParamSteps {
    let hop = thresholds::PARAM_STEP_BLOCK_SAMPLES;
    let floor = f64::from(thresholds::PARAM_STEP_FLOOR_RMS);

    // Prefix sums of the squared signal give any window's RMS in constant time.
    let mut cum = Vec::with_capacity(samples.len() + 1);
    cum.push(0.0_f64);
    for &s in samples {
        let last = cum[cum.len() - 1];
        cum.push(last + f64::from(s) * f64::from(s));
    }
    let rms = |from: usize, to: usize| ((cum[to] - cum[from]) / (to - from) as f64).sqrt();

    let mut count = 0;
    let mut positions = Vec::new();
    let mut max_step_db = 0.0_f64;
    // Current event: (last flagged hop, best dB, hop of best dB).
    let mut run: Option<(usize, f64, usize)> = None;
    let finish = |run: (usize, f64, usize), count: &mut usize, positions: &mut Vec<usize>| {
        *count += 1;
        if positions.len() < thresholds::PARAM_STEP_MAX_POSITIONS {
            positions.push(run.2 * hop);
        }
    };
    let mut t = win;
    while t + win <= samples.len() {
        let (before, after) = (rms(t - win, t), rms(t, t + win));
        let i = t / hop;
        t += hop;
        if before < floor || after < floor {
            continue;
        }
        let db = (20.0 * (after / before).log10()).abs();
        max_step_db = max_step_db.max(db);
        if db <= thresholds::PARAM_STEP_MAX_DB {
            continue;
        }
        run = match run {
            Some((last, best, at)) if i - last <= thresholds::PARAM_STEP_MERGE_BLOCKS => {
                Some(if db > best { (i, db, i) } else { (i, best, at) })
            }
            Some(done) => {
                finish(done, &mut count, &mut positions);
                Some((i, db, i))
            }
            None => Some((i, db, i)),
        };
    }
    if let Some(done) = run {
        finish(done, &mut count, &mut positions);
    }
    ParamSteps {
        count,
        positions,
        max_step_db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_dsp::gain::{Gain, GainParams};
    use asperitas_dsp::processor::Processor;
    use std::f32::consts::TAU;

    const LEN: usize = 9_600;
    const STEP_AT: usize = 4_800 + 17; // deliberately not block-aligned

    /// -20 dBFS 1 kHz sine through Gain, changing to `to_db` at STEP_AT; `snap` skips smoothing.
    /// A 10.4 dB step keeps the peak (0.1 * 3.31) below the output clamp, so the step is not flattened.
    fn run(to_db: f32, snap: bool) -> Vec<f32> {
        run_at(1000.0, to_db, snap)
    }

    fn run_at(hz: f32, to_db: f32, snap: bool) -> Vec<f32> {
        let mut gain = Gain::default();
        gain.set_sample_rate(48_000.0);
        gain.set_params(&GainParams { gain_db: 0.0 });
        gain.reset();
        (0..LEN)
            .map(|n| {
                if n == STEP_AT {
                    gain.set_params(&GainParams { gain_db: to_db });
                    if snap {
                        gain.reset();
                    }
                }
                let x = 0.1 * (TAU * hz * n as f32 / 48_000.0).sin();
                gain.tick([x, x])[0]
            })
            .collect()
    }

    #[test]
    fn unsmoothed_step_is_flagged_at_the_step() {
        for to_db in [10.4, -10.4] {
            let r = detect_param_steps(&run(to_db, true));
            assert_eq!(r.count, 1, "{to_db} dB: {r:?}");
            let p = r.positions[0];
            assert!(
                p.abs_diff(STEP_AT) <= 2 * thresholds::PARAM_STEP_BLOCK_SAMPLES,
                "{to_db} dB: position {p} vs {STEP_AT}"
            );
        }
    }

    #[test]
    fn smoothed_step_passes() {
        for to_db in [10.4, -10.4] {
            let r = detect_param_steps(&run(to_db, false));
            assert_eq!(r.count, 0, "{to_db} dB: {r:?}");
        }
    }

    #[test]
    fn limit_sits_between_smoothed_and_unsmoothed_with_margin() {
        for to_db in [10.4, -10.4] {
            let snapped = detect_param_steps(&run(to_db, true)).max_step_db;
            let smooth = detect_param_steps(&run(to_db, false)).max_step_db;
            eprintln!("{to_db} dB: snapped {snapped:.2} dB, smoothed {smooth:.2} dB");
            assert!(snapped >= 1.5 * thresholds::PARAM_STEP_MAX_DB, "{snapped}");
            assert!(smooth <= thresholds::PARAM_STEP_MAX_DB / 1.25, "{smooth}");
        }
    }

    #[test]
    fn steady_decaying_tones_report_nothing_at_any_pitch() {
        for hz in [98.0_f32, 110.0, 147.0, 196.0, 220.0, 330.0, 440.0, 1000.0] {
            let x: Vec<f32> = (0..144_000)
                .map(|n| {
                    let t = n as f32 / 48_000.0;
                    0.1 * (-2.0 * t).exp() * (TAU * hz * t).sin()
                })
                .collect();
            let r = detect_param_steps(&x);
            eprintln!("{hz} Hz: {:.2} dB", r.max_step_db);
            assert_eq!(r.count, 0, "{hz} Hz: {r:?}");
            assert!(
                r.max_step_db <= thresholds::PARAM_STEP_MAX_DB / 1.5,
                "{hz} Hz: {r:?}"
            );
        }
    }

    #[test]
    fn snapped_step_is_flagged_on_low_carriers() {
        for hz in [110.0, 220.0, 440.0] {
            for to_db in [10.4, -10.4] {
                let r = detect_param_steps(&run_at(hz, to_db, true));
                eprintln!("{hz} Hz {to_db}: {:?}", r);
                assert_eq!(r.count, 1, "{hz} Hz {to_db} dB: {r:?}");
                assert!(
                    r.positions[0].abs_diff(STEP_AT) <= 2 * thresholds::PARAM_STEP_BLOCK_SAMPLES
                );
                let smooth = detect_param_steps(&run_at(hz, to_db, false));
                assert_eq!(smooth.count, 0, "{hz} Hz {to_db} dB smoothed: {smooth:?}");
            }
        }
    }

    #[test]
    fn steady_sine_and_silence_report_nothing() {
        let steady = detect_param_steps(&run(0.0, false));
        assert_eq!(steady.count, 0);
        assert!(steady.max_step_db < 0.5, "{}", steady.max_step_db);
        let quiet = detect_param_steps(&vec![0.0; LEN]);
        assert_eq!((quiet.count, quiet.max_step_db), (0, 0.0));
    }
}
