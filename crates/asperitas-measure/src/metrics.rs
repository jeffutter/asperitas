//! Objective metrics of an aligned loopback capture, as serde JSON.
//!
//! [`analyze`] reports peak and RMS level, runs of silence, sample-discontinuity candidates, and a
//! spectral spur report. Levels are normalized against [`thresholds::LOOP_GAIN_DB`], the baseline
//! baseline, so `deviation_db` says how far this run is from the known-good rig.
//!
//! # What a deviation can and cannot be attributed to
//!
//! The capture is the stimulus after the DAC, the cable and the ADC. A level, noise or spur
//! deviation is therefore attributable to the output converter and input converter COMBINED; it
//! cannot be split between them. Faults common to both sides are invisible: a shared clock error
//! scales stimulus and capture alike, and a left/right channel swap leaves a single-lane loopback
//! looking healthy (the loop-gain criterion). A clean report is evidence about the loop, not proof of
//! the Pod's individual converters.
//!
//! # Discontinuity candidates
//!
//! The capture is compared with the delayed, gain-scaled stimulus (fractional delay by windowed
//! sinc, delay taken from the [`Alignment`]). A candidate is a sample where that residual jumps
//! by more than [`thresholds::DISCONTINUITY_JUMP_LSB`]; the detector assumes nothing about the
//! stimulus shape, so it works for sine, sweep and pulse. The spur report is only meaningful for
//! a tonal stimulus: the "fundamental" is the strongest bin.

use std::f64::consts::PI;

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};

use crate::align::Alignment;
use crate::capture::{Capture, FULL_SCALE};
use crate::thresholds;

const INTERP_TAPS: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub samples: usize,
    pub level: Level,
    pub silence: Silence,
    pub discontinuities: Discontinuities,
    pub spurs: Spurs,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub peak_lsb: u32,
    pub peak_dbfs: f64,
    pub rms_lsb: f64,
    pub rms_dbfs: f64,
    /// Capture RMS over stimulus RMS, dB.
    pub gain_db: f64,
    /// `gain_db` minus the baseline baseline loop gain, dB.
    pub deviation_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Silence {
    pub run_count: usize,
    pub longest_run_samples: usize,
    pub total_silent_samples: usize,
    /// Start sample of each run, in capture order.
    pub run_starts: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Discontinuities {
    /// Exact number of merged events.
    pub count: usize,
    /// Capture sample index of each event, capped at `DISCONTINUITY_MAX_POSITIONS`.
    pub positions: Vec<usize>,
    pub max_jump_lsb: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spurs {
    pub fundamental_hz: f64,
    pub fundamental_dbfs: f64,
    /// Largest non-fundamental peaks, strongest first.
    pub peaks: Vec<Spur>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spur {
    pub freq_hz: f64,
    pub dbfs: f64,
    /// Relative to the fundamental.
    pub dbc: f64,
}

impl Metrics {
    #[cfg(test)]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Metrics serializes")
    }

    /// Worst spur relative to the fundamental, dB; `-inf` when there is none.
    pub fn worst_spur_dbc(&self) -> f64 {
        self.spurs
            .peaks
            .first()
            .map_or(f64::NEG_INFINITY, |s| s.dbc)
    }
}

/// Compute all metrics. `stimulus` must cover the capture (alignment already requires this).
pub fn analyze(capture: &Capture, stimulus: &[f32], alignment: &Alignment) -> Metrics {
    let x: Vec<f64> = capture.samples.iter().map(|&s| f64::from(s)).collect();
    let n = x.len();
    let level = level(&x, stimulus);
    let gain = 10f64.powf(level.gain_db / 20.0);
    Metrics {
        samples: n,
        silence: silence(&capture.samples),
        discontinuities: discontinuities(&x, stimulus, alignment, GainFit::Fixed(gain)),
        spurs: spurs(&x, f64::from(capture.sample_rate_hz)),
        level,
    }
}

fn dbfs(lsb: f64) -> f64 {
    20.0 * (lsb.max(1e-12) / FULL_SCALE).log10()
}

fn level(x: &[f64], stimulus: &[f32]) -> Level {
    let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let rms = (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt();
    let m = x.len().min(stimulus.len()).max(1);
    let stim_rms_lsb = (stimulus[..m]
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        / m as f64)
        .sqrt()
        * FULL_SCALE;
    let gain_db = 20.0 * (rms / stim_rms_lsb.max(1e-12)).max(1e-12).log10();
    Level {
        peak_lsb: peak as u32,
        peak_dbfs: dbfs(peak),
        rms_lsb: rms,
        rms_dbfs: dbfs(rms),
        gain_db,
        deviation_db: gain_db - thresholds::LOOP_GAIN_DB,
    }
}

pub(crate) fn silence(samples: &[i16]) -> Silence {
    let mut s = Silence {
        run_count: 0,
        longest_run_samples: 0,
        total_silent_samples: 0,
        run_starts: Vec::new(),
    };
    let mut start = None;
    let close = |s: &mut Silence, from: usize, to: usize| {
        let len = to - from;
        if len >= thresholds::SILENCE_MIN_RUN_SAMPLES {
            s.run_count += 1;
            s.longest_run_samples = s.longest_run_samples.max(len);
            s.total_silent_samples += len;
            s.run_starts.push(from);
        }
    };
    for (i, &v) in samples.iter().enumerate() {
        let quiet = v.unsigned_abs() <= thresholds::SILENCE_LSB.unsigned_abs();
        match (quiet, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                close(&mut s, from, i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        close(&mut s, from, samples.len());
    }
    s
}

/// Windowed-sinc taps for a fractional delay `frac` in [0, 1): `y[n] = sum_k h[k] x[n - k]`
/// approximates `x(n - frac)`.
fn frac_delay_taps(frac: f64) -> [f64; INTERP_TAPS] {
    let centre = (INTERP_TAPS / 2 - 1) as f64;
    let mut h = [0.0; INTERP_TAPS];
    for (k, t) in h.iter_mut().enumerate() {
        let x = k as f64 - centre - frac;
        let sinc = if x.abs() < 1e-12 {
            1.0
        } else {
            (PI * x).sin() / (PI * x)
        };
        let w = 0.5 * (1.0 + (PI * x / (INTERP_TAPS as f64 / 2.0)).cos());
        *t = sinc * w;
    }
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|t| *t /= sum);
    h
}

/// How the stimulus model is scaled when looking for discontinuities.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GainFit {
    /// One gain for the whole capture (a steady parameter).
    Fixed(f64),
    /// Least-squares gain refitted per block of this many samples, so a parameter that moves
    /// slowly (a knob sweep) leaves a residual that stays small; a dropout or a snapped step
    /// still jumps within the block.
    Tracking { block: usize },
}

/// Stimulus lane scaled to capture LSB, delayed per alignment window.
struct DelayedModel<'a> {
    stimulus: &'a [f32],
    al: &'a Alignment,
    taps: [f64; INTERP_TAPS],
    taps_window: usize,
    base: i64,
}

impl<'a> DelayedModel<'a> {
    fn new(stimulus: &'a [f32], al: &'a Alignment) -> Self {
        Self {
            stimulus,
            al,
            taps: [0.0; INTERP_TAPS],
            taps_window: usize::MAX,
            base: 0,
        }
    }

    /// `(model sample, bottom tap index)`; `None` until every interpolation tap is in range.
    fn at(&mut self, n: usize) -> Option<(f64, i64)> {
        let w = (n / self.al.window_samples).min(self.al.delays.len() - 1);
        if w != self.taps_window {
            self.taps_window = w;
            let d = self.al.delays[w];
            let di = d.floor();
            self.base = di as i64;
            self.taps = frac_delay_taps(d - di);
        }
        // Stimulus index for tap k is n - base - k + centre.
        let centre = (INTERP_TAPS / 2 - 1) as i64;
        let top = n as i64 - self.base + centre;
        let bottom = top - (INTERP_TAPS as i64 - 1);
        if bottom < 0 || top >= self.stimulus.len() as i64 {
            return None;
        }
        let m: f64 = self
            .taps
            .iter()
            .enumerate()
            .map(|(k, h)| h * f64::from(self.stimulus[(top - k as i64) as usize]))
            .sum::<f64>()
            * FULL_SCALE;
        Some((m, bottom))
    }
}

pub(crate) fn discontinuities(
    x: &[f64],
    stimulus: &[f32],
    al: &Alignment,
    fit: GainFit,
) -> Discontinuities {
    let mut out = Discontinuities {
        count: 0,
        positions: Vec::new(),
        max_jump_lsb: 0.0,
    };
    let mut prev = 0.0;
    let mut last_event: Option<usize> = None;
    let mut model = DelayedModel::new(stimulus, al);
    let block = match fit {
        GainFit::Fixed(_) => thresholds::FIXED_GAIN_BLOCK_SAMPLES,
        GainFit::Tracking { block } => block.max(1),
    };
    let mut buf: Vec<Option<(f64, i64)>> = Vec::with_capacity(block);
    for start in (0..x.len()).step_by(block) {
        let end = (start + block).min(x.len());
        buf.clear();
        buf.extend((start..end).map(|n| model.at(n)));
        let gain = match fit {
            GainFit::Fixed(g) => g,
            GainFit::Tracking { .. } => {
                let (mut xm, mut mm) = (0.0, 0.0);
                for (n, m) in (start..end).zip(&buf) {
                    if let Some((m, _)) = m {
                        xm += x[n] * m;
                        mm += m * m;
                    }
                }
                if mm > 0.0 {
                    xm / mm
                } else {
                    0.0
                }
            }
        };
        for (n, m) in (start..end).zip(&buf) {
            let Some((m, bottom)) = m else { continue };
            let r = x[n] - m * gain;
            let jump = (r - prev).abs();
            prev = r;
            if n == 0 || *bottom == 0 {
                continue;
            }
            out.max_jump_lsb = out.max_jump_lsb.max(jump);
            if jump > thresholds::DISCONTINUITY_JUMP_LSB {
                let merged =
                    last_event.is_some_and(|l| n - l <= thresholds::DISCONTINUITY_MERGE_SAMPLES);
                last_event = Some(n);
                if !merged {
                    out.count += 1;
                    if out.positions.len() < thresholds::DISCONTINUITY_MAX_POSITIONS {
                        out.positions.push(n);
                    }
                }
            }
        }
    }
    out
}

fn spurs(x: &[f64], sample_rate: f64) -> Spurs {
    let len = thresholds::SPUR_FFT_SAMPLES;
    let bins = len / 2;
    let window: Vec<f64> = (0..len)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / len as f64).cos())
        .collect();
    let wsum: f64 = window.iter().sum();
    let fft = FftPlanner::<f64>::new().plan_fft_forward(len);
    let mut power = vec![0.0f64; bins];
    let mut frames = 0usize;
    let mut buf = vec![Complex::new(0.0, 0.0); len];
    for frame in x.chunks_exact(len) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = Complex::new(frame[i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        for (p, c) in power.iter_mut().zip(&buf) {
            *p += c.norm_sqr();
        }
        frames += 1;
    }
    if frames == 0 {
        return Spurs {
            fundamental_hz: 0.0,
            fundamental_dbfs: f64::NEG_INFINITY,
            peaks: Vec::new(),
        };
    }
    // Amplitude in LSB of a sinusoid centred in each bin, from the mean power across frames.
    let amp: Vec<f64> = power
        .iter()
        .map(|p| 2.0 * (p / frames as f64).sqrt() / wsum)
        .collect();
    let bin_hz = sample_rate / len as f64;
    let ex = thresholds::SPUR_EXCLUDE_BINS;
    // Skip DC and its leakage; DC is reported by the baseline, not as a spur.
    let first = ex + 1;
    let fund = (first..bins)
        .max_by(|&a, &b| amp[a].total_cmp(&amp[b]))
        .unwrap_or(first);
    let fund_amp = amp[fund];
    let mut peaks: Vec<(usize, f64)> = (first..bins - 1)
        .filter(|&b| b.abs_diff(fund) > ex && amp[b] >= amp[b - 1] && amp[b] >= amp[b + 1])
        .map(|b| (b, amp[b]))
        .collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    peaks.truncate(thresholds::SPUR_REPORT_COUNT);
    Spurs {
        fundamental_hz: fund as f64 * bin_hz,
        fundamental_dbfs: dbfs(fund_amp),
        peaks: peaks
            .into_iter()
            .map(|(b, a)| Spur {
                freq_hz: b as f64 * bin_hz,
                dbfs: dbfs(a),
                dbc: 20.0 * (a / fund_amp).log10(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::align::{align, assert_stable};
    use crate::stimulus::Stimulus;
    use asperitas_dsp::stimulus::SineParams;

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

    fn run(c: &Capture) -> Metrics {
        let st = sine(c.samples.len());
        let a = align(&c.samples, &st).unwrap();
        assert_stable(&a).unwrap();
        analyze(c, &st, &a)
    }

    fn assert_healthy(m: &Metrics) {
        eprintln!("{}", m.to_json());
        assert!(
            m.level.deviation_db.abs() <= thresholds::LEVEL_DEVIATION_TOL_DB,
            "deviation {}",
            m.level.deviation_db
        );
        assert_eq!(m.silence.run_count, 0);
        assert_eq!(m.discontinuities.count, 0, "{:?}", m.discontinuities);
        assert!(m.worst_spur_dbc() <= thresholds::SPUR_MAX_DBC);
    }

    #[test]
    fn fixture_2026_10_07_300s() {
        assert_healthy(&run(&load("TASK-038.05/2026-10-07-rig-sine-300s")));
    }

    #[test]
    fn fixture_2026_10_15_bootloaded() {
        assert_healthy(&run(&load(
            "TASK-038.04.07.04/2026-10-15-rig-sine-300s-bootloaded",
        )));
    }

    /// 10 s synthetic capture: the stimulus, 37 samples late, at the baseline gain.
    fn synthetic() -> Capture {
        let n = 10 * 48_000;
        let st = sine(n);
        let g = 10f64.powf(thresholds::LOOP_GAIN_DB / 20.0);
        let samples = (0..n)
            .map(|i| {
                let v = i.checked_sub(37).map_or(0.0, |j| f64::from(st[j]));
                (v * g * FULL_SCALE).round() as i16
            })
            .collect();
        Capture {
            samples,
            sample_rate_hz: 48_000,
            blocks: 1,
        }
    }

    #[test]
    fn synthetic_baseline_is_healthy() {
        assert_healthy(&run(&synthetic()));
    }

    #[test]
    fn injected_step_is_a_discontinuity() {
        let mut c = synthetic();
        for s in &mut c.samples[200_000..] {
            *s = s.saturating_add(300);
        }
        let m = run(&c);
        assert_eq!(m.discontinuities.count, 1);
        assert_eq!(m.discontinuities.positions, vec![200_000]);
    }

    #[test]
    fn injected_dropout_is_silence_and_discontinuity() {
        let mut c = synthetic();
        // Start at a sine peak region so the dropout is a real jump: 1 kHz period is 48 samples.
        c.samples[300_010..300_210].fill(0);
        let m = run(&c);
        assert_eq!(m.silence.run_count, 1);
        assert!(m.silence.longest_run_samples >= 150);
        assert!(m.discontinuities.count >= 1);
        assert!(m
            .discontinuities
            .positions
            .iter()
            .any(|p| (300_000..300_300).contains(p)));
    }

    #[test]
    fn injected_spur_is_reported() {
        let mut c = synthetic();
        // -50 dBc at 3 kHz (bin-centred: 3000 Hz is 1024 * 48000/16384).
        let amp = 0.1
            * FULL_SCALE
            * 10f64.powf(-50.0 / 20.0)
            * 10f64.powf(thresholds::LOOP_GAIN_DB / 20.0);
        for (i, s) in c.samples.iter_mut().enumerate() {
            let t = (2.0 * PI * 3000.0 * i as f64 / 48_000.0).sin();
            *s = s.saturating_add((amp * t).round() as i16);
        }
        let m = run(&c);
        let top = &m.spurs.peaks[0];
        assert!((top.freq_hz - 3000.0).abs() < 3.0, "{top:?}");
        assert!(top.dbc > thresholds::SPUR_MAX_DBC);
        assert!((top.dbc + 50.0).abs() < 1.5, "{top:?}");
    }

    #[test]
    fn json_round_trips() {
        let m = run(&synthetic());
        let back: Metrics = serde_json::from_str(&m.to_json()).unwrap();
        assert_eq!(back.samples, m.samples);
    }
}
