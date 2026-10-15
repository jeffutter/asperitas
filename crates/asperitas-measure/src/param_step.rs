//! Detector for audible steps caused by parameter changes.
//!
//! A parameter change that is not smoothed shows up as an abrupt change in signal level. The
//! detector splits the signal into [`thresholds::PARAM_STEP_BLOCK_SAMPLES`] blocks, takes block
//! RMS, and flags each boundary where the level changes by more than
//! [`thresholds::PARAM_STEP_MAX_DB`]. Flags within [`thresholds::PARAM_STEP_MERGE_BLOCKS`] merge
//! into one event. Comparisons also span two blocks, so a step landing mid-block is not diluted. Blocks below the
//! noise floor are never compared.
//!
//! The tests prove the detector can fire: the same gain change flags when snapped and passes
//! through the smoothed `Gain` path.

use serde::{Deserialize, Serialize};

use crate::thresholds;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamSteps {
    /// Exact number of merged events.
    pub count: usize,
    /// Sample index of the start of the block after each event's first flagged boundary, capped
    /// at `PARAM_STEP_MAX_POSITIONS`.
    pub positions: Vec<usize>,
    /// Largest adjacent-block level change seen, dB (0 when no comparable blocks).
    pub max_step_db: f64,
}

/// Find parameter-step events in a mono signal (full scale 1.0).
pub fn detect_param_steps(samples: &[f32]) -> ParamSteps {
    let block = thresholds::PARAM_STEP_BLOCK_SAMPLES;
    let rms: Vec<f64> = samples
        .chunks_exact(block)
        .map(|b| {
            let ms = b.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / block as f64;
            ms.sqrt()
        })
        .collect();
    let floor = f64::from(thresholds::PARAM_STEP_FLOOR_RMS);

    let mut count = 0;
    let mut positions = Vec::new();
    let mut max_step_db = 0.0_f64;
    let mut last_flag: Option<usize> = None;
    for i in 1..rms.len() {
        // Lag 1 catches a step on a block boundary; lag 2 skips the block a mid-block step
        // straddles, whose RMS is only partway between the two levels.
        let mut db = 0.0_f64;
        for lag in 1..=2.min(i) {
            if rms[i] < floor || rms[i - lag] < floor {
                continue;
            }
            db = db.max((20.0 * (rms[i] / rms[i - lag]).log10()).abs());
        }
        max_step_db = max_step_db.max(db);
        if db <= thresholds::PARAM_STEP_MAX_DB {
            continue;
        }
        let merged = last_flag.is_some_and(|l| i - l <= thresholds::PARAM_STEP_MERGE_BLOCKS);
        last_flag = Some(i);
        if merged {
            continue;
        }
        count += 1;
        if positions.len() < thresholds::PARAM_STEP_MAX_POSITIONS {
            positions.push(i * block);
        }
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
                let x = 0.1 * (TAU * 1000.0 * n as f32 / 48_000.0).sin();
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
            assert!(smooth <= thresholds::PARAM_STEP_MAX_DB / 1.5, "{smooth}");
        }
    }

    #[test]
    fn steady_sine_and_silence_report_nothing() {
        let steady = detect_param_steps(&run(0.0, false));
        assert_eq!(steady.count, 0);
        assert!(steady.max_step_db < 0.01, "{}", steady.max_step_db);
        let quiet = detect_param_steps(&vec![0.0; LEN]);
        assert_eq!((quiet.count, quiet.max_step_db), (0, 0.0));
    }
}
