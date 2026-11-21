//! Accuracy of the pitch tracker on recorded and synthetic signals. See
//! `docs/reference/pitch-accuracy.md`.
//!
//! Three parts, all host-only and with no I/O:
//!
//! - [`run_tracker`] feeds a signal through `asperitas_dsp::pitch::PitchTracker` and records its
//!   estimate and confidence once per analysis hop.
//! - [`reference_track`] is an independent offline analysis of the same samples: normalised
//!   autocorrelation over a 4096-sample window at the full sample rate, which is longer and
//!   finer than the tracker's decimated 384-sample window. It is a reference, not ground truth:
//!   it can be wrong on a signal the tracker is also wrong on. The synthetic generators carry
//!   exact truth instead.
//! - [`score`] compares the two, frame by frame, and [`detect_onsets`] finds note onsets for the
//!   latency figure.
//!
//! Every track and reference frame `k` is stamped with the time `(k + 1) * HOP / rate`: the tracker
//! has heard the signal up to that time, and the reference window is centred on it. The tracker
//! therefore lags the reference by about half of its own window; that lag is part of what the
//! scores measure.

use asperitas_dsp::pitch::{PitchParams, PitchTracker, PITCH_HOP};
use rustfft::{num_complex::Complex, FftPlanner};

/// Samples between frames: the tracker's analysis hop.
pub const HOP: usize = PITCH_HOP;
/// Reference analysis window in samples (85 ms at 48 kHz, three periods of 38 Hz).
const REF_WINDOW: usize = 4096;
const REF_FFT: usize = 2 * REF_WINDOW;
/// Reference search range in Hz.
const REF_MIN_HZ: f64 = 38.0;
const REF_MAX_HZ: f64 = 2400.0;
/// The reference calls a frame voiced when its peak clarity reaches this.
pub const REF_CLARITY: f64 = 0.9;
/// A key maximum is chosen when it reaches this fraction of the highest one.
const KEY_FRACTION: f64 = 0.9;
/// Level below which a window counts as silence, in dBFS RMS.
pub const PRESENT_FLOOR_DB: f64 = -50.0;
/// An onset is a rise of this many dB within [`ONSET_RISE_BLOCKS`] blocks.
const ONSET_RISE_DB: f64 = 20.0;
const ONSET_RISE_BLOCKS: usize = 8;
/// Shortest gap between two onsets, in blocks (150 ms at 48 kHz).
const ONSET_HOLDOFF_BLOCKS: usize = 56;
/// How long after an onset the reference may start being voiced, and how long the tracker has to
/// agree with it, in seconds.
const ONSET_REF_WINDOW_S: f64 = 0.4;
const ONSET_TRACK_WINDOW_S: f64 = 0.6;

/// One frame of the reference or of exact synthetic truth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefFrame {
    pub t_s: f64,
    /// Fundamental in Hz, `0.0` when not voiced.
    pub hz: f64,
    pub voiced: bool,
    /// Window RMS in dBFS.
    pub level_db: f64,
}

/// One frame of tracker output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFrame {
    pub t_s: f64,
    /// Held estimate in Hz, `0.0` before the first.
    pub hz: f64,
    pub conf: f64,
}

/// What a frame's estimate is, against the truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Within the tolerance of the truth.
    Correct,
    /// Within the tolerance of 2x or 0.5x the truth.
    Octave,
    /// Within the tolerance of 3x, 1.5x, 4x, 2/3, 1/3 or 1/4 the truth.
    Harmonic,
    /// An estimate, but none of the above.
    Other,
    /// No estimate was held.
    None,
}

/// Pitch difference `est` against `truth` in cents.
pub fn cents(est: f64, truth: f64) -> f64 {
    1200.0 * (est / truth).log2()
}

/// Classify `est` against `truth` with `tolerance_cents` as the width of every match.
pub fn classify(est: f64, truth: f64, tolerance_cents: f64) -> Outcome {
    if !(est.is_finite() && est > 0.0) {
        return Outcome::None;
    }
    let c = cents(est, truth);
    if c.abs() <= tolerance_cents {
        return Outcome::Correct;
    }
    let near = |ratio: f64| (c - 1200.0 * ratio.log2()).abs() <= tolerance_cents;
    if near(2.0) || near(0.5) {
        Outcome::Octave
    } else if [3.0, 1.5, 4.0, 2.0 / 3.0, 1.0 / 3.0, 0.25]
        .iter()
        .any(|&r| near(r))
    {
        Outcome::Harmonic
    } else {
        Outcome::Other
    }
}

/// Run the tracker over `samples` and return one frame per hop. The tracker is fed a hop at a
/// time; it is equal to feeding the device's 32-sample blocks because it analyses on a fixed
/// sample count.
pub fn run_tracker(samples: &[f32], rate: f32, clarity_threshold: f32) -> Vec<TrackFrame> {
    let mut tracker = PitchTracker::new(rate, &PitchParams { clarity_threshold });
    samples
        .chunks_exact(HOP)
        .enumerate()
        .map(|(k, chunk)| {
            tracker.process_block(chunk);
            TrackFrame {
                t_s: (k + 1) as f64 * HOP as f64 / f64::from(rate),
                hz: f64::from(tracker.estimate_hz()),
                conf: f64::from(tracker.confidence()),
            }
        })
        .collect()
}

/// Window RMS in dBFS of the reference window centred on sample `center`, zero beyond the ends.
/// The mean is over the samples that exist, so a clip edge does not read as quiet.
fn window_level_db(samples: &[f32], center: usize) -> f64 {
    let lo = center.saturating_sub(REF_WINDOW / 2);
    let hi = (center + REF_WINDOW / 2).min(samples.len());
    let n = hi.saturating_sub(lo).max(1);
    let ms: f64 = samples[lo.min(hi)..hi]
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        / n as f64;
    10.0 * (ms + 1e-12).log10()
}

/// Number of frames for `n` samples.
fn frame_count(n: usize) -> usize {
    n / HOP
}

/// Offline reference pitch of `samples`, one frame per hop, centred on the same times as
/// [`run_tracker`]'s frames. The method is McLeod's normalised square difference over a
/// 4096-sample full-rate window with the first-key-maximum-above-0.9 rule and parabolic
/// refinement, so it is the same family as the tracker but with a longer and finer view. A frame
/// is voiced when its level is above [`PRESENT_FLOOR_DB`] and its clarity reaches
/// [`REF_CLARITY`].
pub fn reference_track(samples: &[f32], rate: f32) -> Vec<RefFrame> {
    let rate = f64::from(rate);
    let min_lag = (rate / REF_MAX_HZ).floor().max(2.0) as usize;
    let max_lag = ((rate / REF_MIN_HZ).ceil() as usize).min(REF_WINDOW / 2 + REF_WINDOW / 4);
    let mut planner = FftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(REF_FFT);
    let inv = planner.plan_fft_inverse(REF_FFT);

    let mut buf = vec![Complex::new(0.0, 0.0); REF_FFT];
    let mut prefix = vec![0.0f64; REF_WINDOW + 1];
    let mut nsdf = vec![0.0f64; max_lag + 2];
    (0..frame_count(samples.len()))
        .map(|k| {
            let center = (k + 1) * HOP;
            let level_db = window_level_db(samples, center);
            let t_s = center as f64 / rate;
            let start = center as isize - (REF_WINDOW / 2) as isize;
            for (j, b) in buf.iter_mut().enumerate() {
                let idx = start + j as isize;
                let v = if j < REF_WINDOW && idx >= 0 && (idx as usize) < samples.len() {
                    f64::from(samples[idx as usize])
                } else {
                    0.0
                };
                *b = Complex::new(v, 0.0);
            }
            for j in 0..REF_WINDOW {
                prefix[j + 1] = prefix[j] + buf[j].re * buf[j].re;
            }
            fwd.process(&mut buf);
            for b in buf.iter_mut() {
                *b = Complex::new(b.norm_sqr(), 0.0);
            }
            inv.process(&mut buf);
            for (t, n) in nsdf.iter_mut().enumerate().take(max_lag + 2) {
                let m = prefix[REF_WINDOW - t] + (prefix[REF_WINDOW] - prefix[t]);
                *n = if m > 1e-12 {
                    2.0 * buf[t].re / REF_FFT as f64 / m
                } else {
                    0.0
                };
            }
            let (hz, clarity) = pick_peak(&nsdf, min_lag, max_lag, rate);
            let voiced = level_db >= PRESENT_FLOOR_DB && clarity >= REF_CLARITY && hz > 0.0;
            RefFrame {
                t_s,
                hz: if voiced { hz } else { 0.0 },
                voiced,
                level_db,
            }
        })
        .collect()
}

/// First key maximum at or above [`KEY_FRACTION`] of the highest, refined by a parabola.
/// Returns `(hz, clarity)`, `(0.0, 0.0)` when there is no peak.
fn pick_peak(nsdf: &[f64], min_lag: usize, max_lag: usize, rate: f64) -> (f64, f64) {
    let mut maxima: Vec<(usize, f64)> = Vec::new();
    let mut t = 1;
    // Skip the lobe around lag 0.
    while t <= max_lag && nsdf[t] > 0.0 {
        t += 1;
    }
    while t <= max_lag {
        while t <= max_lag && nsdf[t] <= 0.0 {
            t += 1;
        }
        let mut best: Option<(usize, f64)> = None;
        while t <= max_lag && nsdf[t] > 0.0 {
            if best.is_none_or(|(_, v)| nsdf[t] > v) {
                best = Some((t, nsdf[t]));
            }
            t += 1;
        }
        if let Some((i, v)) = best {
            if i >= min_lag {
                maxima.push((i, v));
            }
        }
    }
    let Some(top) = maxima
        .iter()
        .map(|m| m.1)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))))
    else {
        return (0.0, 0.0);
    };
    let Some(&(idx, _)) = maxima.iter().find(|m| m.1 >= KEY_FRACTION * top) else {
        return (0.0, 0.0);
    };
    let (y0, y1, y2) = (nsdf[idx - 1], nsdf[idx], nsdf[idx + 1]);
    let d = y0 - 2.0 * y1 + y2;
    let (delta, height) = if d < 0.0 {
        let delta = (0.5 * (y0 - y2) / d).clamp(-1.0, 1.0);
        (delta, y1 - 0.25 * (y0 - y2) * delta)
    } else {
        (0.0, y1)
    };
    let lag = idx as f64 + delta;
    (rate / lag, height.clamp(0.0, 1.0))
}

/// Times in seconds of note onsets: the first 128-sample block whose peak-envelope level is above
/// [`PRESENT_FLOOR_DB`] and at least 20 dB above the quietest of the previous eight blocks (the
/// time before the clip counts as silence), at least 150 ms after the previous onset.
pub fn detect_onsets(samples: &[f32], rate: f32) -> Vec<f64> {
    // Peak follower with a 30 ms release: a block RMS swings by tens of dB within one period of a
    // low note, a peak envelope does not.
    let release = (-1.0 / (0.030 * f64::from(rate))).exp();
    let mut env = 0.0f64;
    let levels: Vec<f64> = samples
        .chunks_exact(HOP)
        .map(|c| {
            for &v in c {
                env = f64::from(v).abs().max(env * release);
            }
            20.0 * (env + 1e-6).log10()
        })
        .collect();
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for i in 0..levels.len() {
        if levels[i] < PRESENT_FLOOR_DB {
            continue;
        }
        let prev_min = (0..ONSET_RISE_BLOCKS)
            .map(|d| i.checked_sub(d + 1).map_or(-120.0, |j| levels[j]))
            .fold(f64::MAX, f64::min);
        let held = last.is_some_and(|l| i < l + ONSET_HOLDOFF_BLOCKS);
        if levels[i] - prev_min >= ONSET_RISE_DB && !held {
            onsets.push(i as f64 * HOP as f64 / f64::from(rate));
            last = Some(i);
        }
    }
    onsets
}

/// What to judge a track by.
#[derive(Clone, Copy, Debug)]
pub struct ScoreConfig {
    /// Width of "correct", in cents.
    pub tolerance_cents: f64,
    /// Confidence at which the tracker's estimate counts as current.
    pub clarity_threshold: f64,
}

/// Whether the signal has one right pitch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A single line: accuracy, octave errors and onset latency are scored.
    Monophonic,
    /// A chord or cluster: there is no single right pitch, only the confidence is scored.
    Chordal,
}

/// The numbers for one clip. Fractions are over voiced frames unless stated; `None` when there
/// were no frames to take them over.
#[derive(Clone, Debug, PartialEq)]
pub struct Scores {
    pub kind: Kind,
    pub frames: usize,
    /// Seconds of voiced reference (monophonic only).
    pub voiced_s: f64,
    pub correct: Option<f64>,
    pub within_10c: Option<f64>,
    pub octave_error: Option<f64>,
    pub harmonic_error: Option<f64>,
    pub other_error: Option<f64>,
    pub no_estimate: Option<f64>,
    /// Mean and max tracker confidence on frames where the signal is present but the reference
    /// is not voiced (monophonic: noise and unpitched passages; chordal: every present frame).
    pub unvoiced_conf_mean: Option<f64>,
    pub unvoiced_conf_max: Option<f64>,
    /// Fraction of those frames at or above the clarity threshold: how often the tracker would
    /// act on a pitch that is not there.
    pub unvoiced_confident: Option<f64>,
    /// Onsets found, and how many the tracker never agreed with the reference after.
    pub onsets: usize,
    pub onsets_missed: usize,
    pub latency_median_ms: Option<f64>,
    pub latency_max_ms: Option<f64>,
}

fn frac(n: usize, d: usize) -> Option<f64> {
    (d > 0).then(|| n as f64 / d as f64)
}

/// Score `track` against `truth` (frames pair by index) with the onsets `onsets_s`.
pub fn score(
    track: &[TrackFrame],
    truth: &[RefFrame],
    onsets_s: &[f64],
    kind: Kind,
    cfg: &ScoreConfig,
) -> Scores {
    let n = track.len().min(truth.len());
    let hop_s = if n > 1 {
        track[1].t_s - track[0].t_s
    } else {
        0.0
    };
    let (mut voiced, mut correct, mut w10, mut oct, mut harm, mut other, mut none) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut u_n, mut u_sum, mut u_max, mut u_conf) = (0usize, 0.0f64, 0.0f64, 0usize);
    for (tr, rf) in track[..n].iter().zip(&truth[..n]) {
        let present = rf.level_db >= PRESENT_FLOOR_DB;
        let unvoiced = match kind {
            Kind::Monophonic => present && !rf.voiced,
            Kind::Chordal => present,
        };
        if unvoiced {
            u_n += 1;
            u_sum += tr.conf;
            u_max = u_max.max(tr.conf);
            u_conf += usize::from(tr.conf >= cfg.clarity_threshold);
        }
        if kind == Kind::Monophonic && rf.voiced {
            voiced += 1;
            match classify(tr.hz, rf.hz, cfg.tolerance_cents) {
                Outcome::Correct => {
                    correct += 1;
                    w10 += usize::from(cents(tr.hz, rf.hz).abs() <= 10.0);
                }
                Outcome::Octave => oct += 1,
                Outcome::Harmonic => harm += 1,
                Outcome::Other => other += 1,
                Outcome::None => none += 1,
            }
        }
    }

    let mut latencies = Vec::new();
    let mut found = 0usize;
    if kind == Kind::Monophonic {
        for &t_on in onsets_s {
            let Some(reference) = truth[..n]
                .iter()
                .find(|f| f.voiced && f.t_s >= t_on && f.t_s <= t_on + ONSET_REF_WINDOW_S)
            else {
                continue;
            };
            found += 1;
            let hit = track[..n].iter().find(|f| {
                f.t_s >= t_on
                    && f.t_s <= t_on + ONSET_TRACK_WINDOW_S
                    && f.conf >= cfg.clarity_threshold
                    && classify(f.hz, reference.hz, cfg.tolerance_cents) == Outcome::Correct
            });
            if let Some(f) = hit {
                latencies.push((f.t_s - t_on) * 1000.0);
            }
        }
    }
    latencies.sort_by(|a, b| a.total_cmp(b));
    let median = (!latencies.is_empty()).then(|| {
        let m = latencies.len() / 2;
        if latencies.len() % 2 == 1 {
            latencies[m]
        } else {
            0.5 * (latencies[m - 1] + latencies[m])
        }
    });

    Scores {
        kind,
        frames: n,
        voiced_s: voiced as f64 * hop_s,
        correct: frac(correct, voiced),
        within_10c: frac(w10, voiced),
        octave_error: frac(oct, voiced),
        harmonic_error: frac(harm, voiced),
        other_error: frac(other, voiced),
        no_estimate: frac(none, voiced),
        unvoiced_conf_mean: (u_n > 0).then(|| u_sum / u_n as f64),
        unvoiced_conf_max: (u_n > 0).then_some(u_max),
        unvoiced_confident: frac(u_conf, u_n),
        onsets: found,
        onsets_missed: found - latencies.len(),
        latency_median_ms: median,
        latency_max_ms: latencies.last().copied(),
    }
}

/// A signal and the exact truth for it.
pub struct Synthetic {
    pub samples: Vec<f32>,
    pub truth: Vec<RefFrame>,
    pub kind: Kind,
}

/// Truth frames for `samples` from `pitch_at(t)`, the exact fundamental at time `t` seconds or
/// `None` where the signal is not pitched.
fn truth_from(samples: &[f32], rate: f32, pitch_at: impl Fn(f64) -> Option<f64>) -> Vec<RefFrame> {
    (0..frame_count(samples.len()))
        .map(|k| {
            let center = (k + 1) * HOP;
            let t_s = center as f64 / f64::from(rate);
            let hz = pitch_at(t_s);
            RefFrame {
                t_s,
                hz: hz.unwrap_or(0.0),
                voiced: hz.is_some() && window_level_db(samples, center) >= PRESENT_FLOOR_DB,
                level_db: window_level_db(samples, center),
            }
        })
        .collect()
}

/// Harmonic amplitudes 1, 1/2, 1/3, 1/4 of `phase` (radians), keeping only those below 0.45 of
/// the rate.
fn harmonic_sum(phase: f64, hz: f64, rate: f64) -> f64 {
    (1..=4)
        .filter(|&k| f64::from(k) * hz < 0.45 * rate)
        .map(|k| (f64::from(k) * phase).sin() / f64::from(k))
        .sum()
}

/// Length of the synthetic sweep in seconds.
pub const SWEEP_SECONDS: f64 = 10.0;
/// Sweep range in Hz.
pub const SWEEP_LOW_HZ: f64 = 41.0;
pub const SWEEP_HIGH_HZ: f64 = 1760.0;

/// A 10 s exponential glide from 41 Hz to 1760 Hz with partials at 1/k, truth the glide's
/// instantaneous frequency. The tracker lags a moving pitch, so some of its error here is the
/// glide, not a wrong estimate.
pub fn synthetic_sweep(rate: f32) -> Synthetic {
    let r = f64::from(rate);
    let n = (SWEEP_SECONDS * r) as usize;
    let ratio = SWEEP_HIGH_HZ / SWEEP_LOW_HZ;
    let rate_ln = ratio.ln() / SWEEP_SECONDS;
    let hz_at = |t: f64| SWEEP_LOW_HZ * (rate_ln * t).exp();
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / r;
            let phase =
                2.0 * std::f64::consts::PI * SWEEP_LOW_HZ / rate_ln * ((rate_ln * t).exp() - 1.0);
            (0.25 * harmonic_sum(phase, hz_at(t), r)) as f32
        })
        .collect();
    let truth = truth_from(&samples, rate, |t| Some(hz_at(t)));
    Synthetic {
        samples,
        truth,
        kind: Kind::Monophonic,
    }
}

/// Notes of [`synthetic_notes`] in Hz, spanning E1 to A6.
pub const NOTE_HZ: [f64; 12] = [
    41.2, 55.0, 82.4, 110.0, 164.8, 220.0, 330.0, 440.0, 659.3, 880.0, 1318.5, 1760.0,
];
/// Seconds per note, and the silent tail of each.
const NOTE_S: f64 = 0.5;
const NOTE_GAP_S: f64 = 0.06;

/// Twelve plucked-style notes of half a second each from [`NOTE_HZ`], with a 1 ms attack, partial
/// `k` at `1/k` decaying with time constant `0.4 / k` s, and a silent gap at the end of each, so
/// every note is an onset with an exact pitch.
pub fn synthetic_notes(rate: f32) -> Synthetic {
    let r = f64::from(rate);
    let per = (NOTE_S * r) as usize;
    let mut samples = vec![0.0f32; per * NOTE_HZ.len()];
    for (idx, &hz) in NOTE_HZ.iter().enumerate() {
        let live = ((NOTE_S - NOTE_GAP_S) * r) as usize;
        for j in 0..live {
            let t = j as f64 / r;
            let attack = (t / 0.001).min(1.0);
            let v: f64 = (1..=4)
                .filter(|&k| f64::from(k) * hz < 0.45 * r)
                .map(|k| {
                    let k = f64::from(k);
                    (2.0 * std::f64::consts::PI * k * hz * t).sin() / k * (-t * k / 0.4).exp()
                })
                .sum();
            samples[idx * per + j] = (0.4 * attack * v) as f32;
        }
    }
    let truth = truth_from(&samples, rate, |t| {
        let i = (t / NOTE_S) as usize;
        let into = t - i as f64 * NOTE_S;
        (i < NOTE_HZ.len() && into < NOTE_S - NOTE_GAP_S).then(|| NOTE_HZ[i])
    });
    Synthetic {
        samples,
        truth,
        kind: Kind::Monophonic,
    }
}

/// Three seconds of deterministic white noise at -14 dBFS RMS: no pitch, so every frame is
/// unvoiced and the confidence is the false-confidence figure.
pub fn synthetic_noise(rate: f32) -> Synthetic {
    let n = (3.0 * f64::from(rate)) as usize;
    let mut state = 0x1234_5678u32;
    let samples: Vec<f32> = (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(state >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0) as f32 * 0.35
        })
        .collect();
    let truth = truth_from(&samples, rate, |_| None);
    Synthetic {
        samples,
        truth,
        kind: Kind::Monophonic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;
    const CFG: ScoreConfig = ScoreConfig {
        tolerance_cents: 50.0,
        clarity_threshold: 0.8,
    };

    fn tone(hz: f64, seconds: f64) -> Vec<f32> {
        (0..(seconds * f64::from(RATE)) as usize)
            .map(|i| {
                let ph = 2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE);
                (0.3 * harmonic_sum(ph, hz, f64::from(RATE)) / 1.5) as f32
            })
            .collect()
    }

    fn scored(samples: &[f32], truth: &[RefFrame]) -> Scores {
        let track = run_tracker(samples, RATE, 0.8);
        score(
            &track,
            truth,
            &detect_onsets(samples, RATE),
            Kind::Monophonic,
            &CFG,
        )
    }

    #[test]
    fn a_220_hz_note_scores_correct_with_a_short_latency() {
        let x = tone(220.0, 3.0);
        let truth = truth_from(&x, RATE, |_| Some(220.0));
        let s = scored(&x, &truth);
        assert!(s.correct.unwrap() >= 0.95, "{s:?}");
        assert!(s.octave_error.unwrap() < 0.01, "{s:?}");
        assert_eq!(s.onsets, 1, "{s:?}");
        assert!(s.latency_median_ms.unwrap() < 100.0, "{s:?}");
    }

    #[test]
    fn negative_control_a_wrong_truth_is_reported_as_an_error() {
        let x = tone(220.0, 3.0);
        let semitone = truth_from(&x, RATE, |_| Some(220.0 * 2f64.powf(1.0 / 12.0)));
        let s = scored(&x, &semitone);
        assert!(s.correct.unwrap() < 0.05, "{s:?}");
        assert!(s.other_error.unwrap() > 0.9, "{s:?}");
        assert!(
            s.latency_median_ms.is_none() && s.onsets_missed == s.onsets,
            "{s:?}"
        );

        let up = truth_from(&x, RATE, |_| Some(440.0));
        let s = scored(&x, &up);
        assert!(s.correct.unwrap() < 0.05, "{s:?}");
        assert!(s.octave_error.unwrap() > 0.9, "{s:?}");

        let down = truth_from(&x, RATE, |_| Some(110.0));
        let s = scored(&x, &down);
        assert!(s.octave_error.unwrap() > 0.9, "{s:?}");
    }

    #[test]
    fn silence_and_noise_are_unvoiced_with_low_confidence() {
        let silence = vec![0.0f32; 48_000];
        let truth = truth_from(&silence, RATE, |_| None);
        let s = scored(&silence, &truth);
        assert_eq!(s.voiced_s, 0.0);
        assert!(s.unvoiced_conf_mean.is_none(), "silence is not present");

        let noise = synthetic_noise(RATE);
        let s = scored(&noise.samples, &noise.truth);
        assert_eq!(s.voiced_s, 0.0, "{s:?}");
        assert!(s.unvoiced_conf_mean.unwrap() < 0.6, "{s:?}");
    }

    #[test]
    fn classify_edges() {
        assert_eq!(classify(220.0, 220.0, 50.0), Outcome::Correct);
        assert_eq!(classify(220.0 * 1.0293, 220.0, 50.0), Outcome::Correct);
        assert_eq!(classify(220.0 * 1.0595, 220.0, 50.0), Outcome::Other);
        assert_eq!(classify(440.0, 220.0, 50.0), Outcome::Octave);
        assert_eq!(classify(110.0, 220.0, 50.0), Outcome::Octave);
        assert_eq!(classify(660.0, 220.0, 50.0), Outcome::Harmonic);
        assert_eq!(classify(330.0, 220.0, 50.0), Outcome::Harmonic);
        assert_eq!(classify(0.0, 220.0, 50.0), Outcome::None);
        assert_eq!(classify(f64::NAN, 220.0, 50.0), Outcome::None);
        assert!((cents(440.0, 220.0) - 1200.0).abs() < 1e-9);
    }

    #[test]
    fn reference_recovers_a_sine_within_a_cent_including_bass() {
        for hz in [41.2, 110.0, 440.0, 1760.0] {
            let x: Vec<f32> = (0..48_000)
                .map(|i| {
                    (0.3 * (2.0 * std::f64::consts::PI * hz * i as f64 / 48_000.0).sin()) as f32
                })
                .collect();
            let r = reference_track(&x, RATE);
            let mid = &r[r.len() / 2];
            assert!(mid.voiced, "{hz}: {mid:?}");
            assert!(cents(mid.hz, hz).abs() < 1.0, "{hz}: {mid:?}");
        }
    }

    #[test]
    fn onsets_are_found_per_note_and_the_synthetic_truth_is_voiced_only_in_notes() {
        let s = synthetic_notes(RATE);
        assert_eq!(detect_onsets(&s.samples, RATE).len(), NOTE_HZ.len());
        let gap = s
            .truth
            .iter()
            .find(|f| f.t_s > 0.485 && f.t_s < 0.5)
            .unwrap();
        assert!(!gap.voiced);
    }

    #[test]
    fn scores_are_deterministic() {
        let s = synthetic_notes(RATE);
        assert_eq!(scored(&s.samples, &s.truth), scored(&s.samples, &s.truth));
    }

    /// The tracker's trace over the plucked-style notes drives the root-follow stage: after each
    /// onset the root lands on the snapped played note and holds through the decay. The abrupt cut
    /// at the end of a synthetic note makes the tracker read the octave below with high confidence
    /// for about 60 ms, so the silent gap is not asserted.
    #[test]
    fn root_follow_lands_on_each_plucked_note_and_holds_in_the_decay() {
        use asperitas_dsp::{RootFollow, RootFollowParams};
        let s = synthetic_notes(RATE);
        let track = run_tracker(&s.samples, RATE, 0.8);
        let mut stage = RootFollow::new(RATE);
        stage.set_params(&RootFollowParams {
            tracking: true,
            manual_root_hz: 110.0,
            ..RootFollowParams::default()
        });
        // One tracker frame is HOP samples, four 32-frame blocks of the stage.
        let blocks_per_frame = HOP / 32;
        let root_at_frame_end: Vec<(f64, f64)> = track
            .iter()
            .map(|f| {
                let mut hz = 0.0;
                for _ in 0..blocks_per_frame {
                    hz = stage.process_block(f.hz as f32, f.conf as f32);
                }
                (f.t_s, f64::from(hz))
            })
            .collect();
        let at = |t: f64| {
            root_at_frame_end
                .iter()
                .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
                .unwrap()
                .1
        };
        for (i, &hz) in NOTE_HZ.iter().enumerate() {
            let start = i as f64 * NOTE_S;
            let st = (12.0 * (hz / 440.0).log2()).round();
            let want = 440.0 * 2f64.powf(st / 12.0);
            // Well into the decay and just before the note is cut: on the snapped note.
            for t in [start + 0.30, start + 0.43] {
                let got = at(t);
                assert!(
                    (1200.0 * (got / want).log2()).abs() < 5.0,
                    "note {i} ({hz} Hz, snapped {want}) at {t}: root {got}"
                );
            }
        }
    }
}
