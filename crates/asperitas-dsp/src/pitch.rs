//! Pitch tracker: a monophonic fundamental-frequency estimate with a confidence value.
//!
//! The tracker takes a mono input one sample (or one block) at a time and exposes
//! [`PitchTracker::estimate_hz`] and [`PitchTracker::confidence`]. It is a control source, not
//! audio, so it is not a [`crate::processor::Processor`]; a stereo user holds one per channel or
//! feeds it a mono mix. It does not decide what a caller does with a low-confidence estimate.
//!
//! # Method
//!
//! The detector is McLeod's Pitch Method (McLeod and Wyvill, "A smarter way to find pitch", ICMC
//! 2005). For a window `x` it forms the normalised square difference function
//!
//! ```text
//! n(t) = 2 · Σ x[j] x[j+t] / Σ (x[j]² + x[j+t]²)
//! ```
//!
//! which lies in `[-1, 1]` and is `1` at every lag that is a whole number of periods of a
//! perfectly periodic signal. The *key maxima* are the highest points of each positive lobe after
//! the first zero crossing; the chosen peak is the first key maximum that reaches `0.9` times the
//! highest one (this is what keeps a strong second harmonic from winning), refined by parabolic
//! interpolation. The height of the refined peak is the confidence.
//!
//! It was chosen over YIN because the normalised peak height is already a `0..1` periodicity
//! measure, so confidence costs nothing extra, and because key-maximum picking is the standard
//! defence against octave errors on plucked strings whose upper partials decay at different
//! rates. Both are time-domain methods with the same cost profile.
//!
//! # Two bands
//!
//! A 41 Hz period is 1171 samples at 48 kHz, so a full-rate analysis would cost millions of
//! multiply-adds. The input is therefore analysed in two bands that share one interface:
//!
//! - the low band is low-passed at 1.5 kHz and decimated by 8 (6 kHz), window 384 samples
//!   (64 ms), lags up to 160 (about 38 Hz) and from 8 (750 Hz);
//! - the high band is low-passed at 9 kHz and decimated by 2 (24 kHz), window 160 samples
//!   (6.7 ms), lags 10 to 48 (500 to 2400 Hz).
//!
//! The high band is tried first and is used only when its peak lands at 550 Hz or above with a
//! clarity of at least `0.9`; otherwise the low band decides. The reason is that the low band
//! cannot see a whole period above about 750 Hz, where it would report a lower multiple, while
//! the high band cannot see a bass period at all.
//!
//! The analysis runs once per hop of 128 input samples (four 32-frame blocks, 2.7 ms), so the
//! per-block cost is a small tick of filtering with an analysis in one block in four. All memory
//! is fixed arrays in the struct and nothing allocates. The numbers are designed for 48 kHz; at
//! another rate the lag limits keep their sample counts, so the Hz range scales with the rate.
//!
//! # Output
//!
//! `confidence` is the clarity of the chosen peak after the latest analysis, in `0..1`, and is
//! `0` when the window is below the energy floor (mean square `1e-8`, about -80 dBFS) or has no
//! peak. `estimate_hz` holds the last estimate whose confidence reached the clarity threshold
//! (default [`PITCH_PARAMS`] `clarity_threshold`) and is `0.0` before the first one, so it is
//! always finite and never jumps to a low-confidence value. A caller that wants to know whether
//! the estimate is current compares `confidence` with the threshold.
//!
//! Non-finite input samples count as silence. A DC blocker (about 15 Hz) runs in front of both
//! bands because a DC offset correlates at every lag.
//!
//! Measured accuracy, settle time, onset latency, rejection of noise and chords, and cost are in
//! `docs/reference/pitch-tracker.md`.

use crate::param::{ParamSpec, Taper};
use crate::smooth::{clean, sanitize};
use libm::{cosf, sinf};

/// Parameter table for [`PitchTracker`]; the only source of its range and default.
pub const PITCH_PARAMS: [ParamSpec; 1] = [ParamSpec {
    name: "clarity_threshold",
    unit: "",
    min: 0.5,
    max: 0.99,
    taper: Taper::Linear,
    default: 0.8,
}];

/// Parameters for [`PitchTracker`].
#[derive(Clone, Debug, PartialEq)]
pub struct PitchParams {
    /// Confidence at or above which the held estimate is updated, 0.5..0.99. NaN falls back to
    /// the default; others clamp.
    pub clarity_threshold: f32,
}

impl Default for PitchParams {
    fn default() -> Self {
        Self {
            clarity_threshold: PITCH_PARAMS[0].default,
        }
    }
}

/// Input samples between analyses: four 32-frame blocks.
pub const PITCH_HOP: usize = 128;

/// Mean square below which a window counts as silence.
const ENERGY_FLOOR: f32 = 1e-8;
/// McLeod's `k`: the chosen peak is the first key maximum at least this fraction of the highest.
const PEAK_K: f32 = 0.9;
/// Clarity the high band needs to override the low band.
const HIGH_CLARITY: f32 = 0.9;
/// Lowest frequency the high band may report.
const HIGH_MIN_HZ: f32 = 550.0;
/// Hops of raw input energy kept to judge whether the signal has just ended.
const LEVEL_HOPS: usize = 8;
/// The signal has just ended when the newest hop carries less than this fraction of the mean
/// energy of the last [`LEVEL_HOPS`] hops.
const CUT_FRACTION: f32 = 0.01;
/// Key maxima kept per analysis; more than this in a window means noise.
const MAX_PEAKS: usize = 24;

const LOW_DECIM: usize = 8;
const LOW_WINDOW: usize = 384;
const LOW_MAX_LAG: usize = 160;
const LOW_MIN_LAG: usize = 8;
const LOW_CUTOFF_HZ: f32 = 1500.0;

const HIGH_DECIM: usize = 2;
const HIGH_WINDOW: usize = 160;
const HIGH_MAX_LAG: usize = 48;
const HIGH_MIN_LAG: usize = 10;
const HIGH_CUTOFF_HZ: f32 = 9000.0;

/// One second-order low-pass section (RBJ cookbook), transposed direct form II.
#[derive(Clone, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn low_pass(cutoff_hz: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * core::f32::consts::PI * cutoff_hz / rate;
        let (s, c) = (sinf(w), cosf(w));
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = sanitize(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = sanitize(self.b2 * x - self.a2 * y);
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A peak found in a band: frequency and clarity.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    hz: f32,
    clarity: f32,
}

/// One analysis band: a fourth-order Butterworth low-pass, a decimator, a ring of the most recent
/// `W` decimated samples and the NSDF peak search over `N = max_lag + 2` lags.
#[derive(Clone, Debug)]
struct Band<const W: usize, const N: usize> {
    decim: usize,
    min_lag: usize,
    cutoff_hz: f32,
    lp: [Biquad; 2],
    phase: usize,
    ring: [f32; W],
    head: usize,
}

impl<const W: usize, const N: usize> Band<W, N> {
    fn new(decim: usize, min_lag: usize, cutoff_hz: f32, rate: f32) -> Self {
        let mut b = Self {
            decim,
            min_lag,
            cutoff_hz,
            lp: [
                Biquad::low_pass(cutoff_hz, 0.5412, rate),
                Biquad::low_pass(cutoff_hz, 1.3066, rate),
            ],
            phase: 0,
            ring: [0.0; W],
            head: 0,
        };
        b.set_sample_rate(rate);
        b
    }

    fn set_sample_rate(&mut self, rate: f32) {
        // A cutoff at or above Nyquist would be unstable; hold it below 0.45 of the rate.
        let fc = self.cutoff_hz.min(0.45 * rate);
        self.lp = [
            Biquad::low_pass(fc, 0.5412, rate),
            Biquad::low_pass(fc, 1.3066, rate),
        ];
    }

    fn reset(&mut self) {
        for s in self.lp.iter_mut() {
            s.reset();
        }
        self.phase = 0;
        self.ring = [0.0; W];
        self.head = 0;
    }

    #[inline]
    fn push(&mut self, x: f32) {
        let stage = self.lp[0].tick(x);
        let y = self.lp[1].tick(stage);
        self.phase += 1;
        if self.phase == self.decim {
            self.phase = 0;
            self.ring[self.head] = y;
            self.head = (self.head + 1) % W;
        }
    }

    /// The strongest pitch candidate in the current window at input rate `rate`, or `None` for
    /// silence or when no key maximum exists.
    fn analyse(&self, rate: f32) -> Option<Candidate> {
        // Linearise the ring, oldest sample first.
        let mut x = [0.0f32; W];
        for (i, v) in x.iter_mut().enumerate() {
            *v = self.ring[(self.head + i) % W];
        }
        let total: f32 = x.iter().map(|v| v * v).sum();
        let mean_square = total / W as f32;
        if mean_square.is_nan() || mean_square < ENERGY_FLOOR {
            return None;
        }

        // NSDF over lags 0..N. `a` is the energy of x[0..W-t] and `b` of x[t..W].
        let mut nsdf = [0.0f32; N];
        let (mut a, mut b) = (total, total);
        for (t, out) in nsdf.iter_mut().enumerate() {
            if t > 0 {
                a -= x[W - t] * x[W - t];
                b -= x[t - 1] * x[t - 1];
            }
            let mut r = 0.0f32;
            for j in 0..W - t {
                r += x[j] * x[j + t];
            }
            let m = a + b;
            *out = if m > ENERGY_FLOOR * 1e-3 {
                (2.0 * r / m).clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }

        // Key maxima: the highest point of each positive lobe after the first zero crossing.
        let mut peaks = [(0usize, 0.0f32); MAX_PEAKS];
        let mut count = 0;
        let mut i = 1;
        while i < N && nsdf[i] > 0.0 {
            i += 1;
        }
        while i < N - 1 && count < MAX_PEAKS {
            while i < N && nsdf[i] <= 0.0 {
                i += 1;
            }
            if i >= N {
                break;
            }
            let mut best = i;
            while i < N && nsdf[i] > 0.0 {
                if nsdf[i] > nsdf[best] {
                    best = i;
                }
                i += 1;
            }
            if best < N - 1 && best >= self.min_lag.max(1) && nsdf[best] > 0.0 {
                peaks[count] = (best, nsdf[best]);
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        let highest = peaks[..count].iter().fold(0.0f32, |m, p| m.max(p.1));
        let &(idx, _) = peaks[..count].iter().find(|p| p.1 >= PEAK_K * highest)?;

        // Parabolic interpolation through the peak and its neighbours.
        let (y0, y1, y2) = (nsdf[idx - 1], nsdf[idx], nsdf[idx + 1]);
        let d = y0 - 2.0 * y1 + y2;
        let (delta, height) = if d < -1e-9 {
            let delta = (0.5 * (y0 - y2) / d).clamp(-1.0, 1.0);
            (delta, y1 - 0.25 * (y0 - y2) * delta)
        } else {
            (0.0, y1)
        };
        let lag = idx as f32 + delta;
        if lag.is_nan() || lag <= 0.0 {
            return None;
        }
        let hz = rate / self.decim as f32 / lag;
        Some(Candidate {
            hz,
            clarity: height.clamp(0.0, 1.0),
        })
    }
}

/// A monophonic pitch tracker. See the module docs.
#[derive(Clone, Debug)]
pub struct PitchTracker {
    sample_rate_hz: f32,
    clarity_threshold: f32,
    /// DC blocker: one-pole high-pass `y = x - x1 + R y1`.
    dc_x1: f32,
    dc_y1: f32,
    dc_r: f32,
    low: Band<LOW_WINDOW, { LOW_MAX_LAG + 2 }>,
    high: Band<HIGH_WINDOW, { HIGH_MAX_LAG + 2 }>,
    since_analysis: usize,
    /// Energy of the raw input in the hop being filled, and in the last `LEVEL_HOPS` hops.
    hop_energy: f32,
    hop_history: [f32; LEVEL_HOPS],
    hop_head: usize,
    estimate: f32,
    confidence: f32,
}

fn dc_coefficient(rate: f32) -> f32 {
    1.0 - 2.0 * core::f32::consts::PI * 15.0 / rate
}

impl PitchTracker {
    /// A tracker at `sample_rate_hz` with `params` in force, no estimate and zero confidence.
    pub fn new(sample_rate_hz: f32, params: &PitchParams) -> Self {
        let rate = if sample_rate_hz.is_finite() && sample_rate_hz > 0.0 {
            sample_rate_hz
        } else {
            48_000.0
        };
        let mut t = Self {
            sample_rate_hz: rate,
            clarity_threshold: PITCH_PARAMS[0].default,
            dc_x1: 0.0,
            dc_y1: 0.0,
            dc_r: dc_coefficient(rate),
            low: Band::new(LOW_DECIM, LOW_MIN_LAG, LOW_CUTOFF_HZ, rate),
            high: Band::new(HIGH_DECIM, HIGH_MIN_LAG, HIGH_CUTOFF_HZ, rate),
            since_analysis: 0,
            hop_energy: 0.0,
            hop_history: [0.0; LEVEL_HOPS],
            hop_head: 0,
            estimate: 0.0,
            confidence: 0.0,
        };
        t.set_params(params);
        t
    }

    /// Retarget to a new sample rate; a rate that is not positive and finite is ignored. Filter
    /// state and the analysis windows are kept, so call [`reset`](Self::reset) after a change
    /// of rate on a running stream.
    pub fn set_sample_rate(&mut self, hz: f32) {
        if hz.is_finite() && hz > 0.0 {
            self.sample_rate_hz = hz;
            self.dc_r = dc_coefficient(hz);
            self.low.set_sample_rate(hz);
            self.high.set_sample_rate(hz);
        }
    }

    /// Set the clarity threshold. The held estimate and confidence are kept.
    pub fn set_params(&mut self, params: &PitchParams) {
        self.clarity_threshold = clean(params.clarity_threshold, &PITCH_PARAMS[0]);
    }

    /// Feed one input sample. A non-finite sample counts as silence.
    #[inline]
    pub fn tick(&mut self, x: f32) {
        let x = if x.is_finite() { x } else { 0.0 };
        let y = sanitize(x - self.dc_x1 + self.dc_r * self.dc_y1);
        self.dc_x1 = x;
        self.dc_y1 = y;
        self.hop_energy += x * x;
        self.low.push(y);
        self.high.push(y);
        self.since_analysis += 1;
        if self.since_analysis >= PITCH_HOP {
            self.since_analysis = 0;
            self.analyse();
        }
    }

    /// Feed a block of input samples; equal to calling [`tick`](Self::tick) on each.
    pub fn process_block(&mut self, input: &[f32]) {
        for &x in input {
            self.tick(x);
        }
    }

    /// Whether the input has just stopped: the hop that ended now holds almost none of the energy
    /// of the recent hops. Measured on the raw input because the DC blocker and the band filters
    /// ring on after a hard cut, which keeps their windows from looking silent.
    fn just_cut(&mut self) -> bool {
        self.hop_history[self.hop_head] = self.hop_energy;
        self.hop_head = (self.hop_head + 1) % LEVEL_HOPS;
        let newest = self.hop_energy;
        self.hop_energy = 0.0;
        let mean = self.hop_history.iter().sum::<f32>() / LEVEL_HOPS as f32;
        newest <= CUT_FRACTION * mean
    }

    fn analyse(&mut self) {
        let rate = self.sample_rate_hz;
        let high = self
            .high
            .analyse(rate)
            .filter(|c| c.hz >= HIGH_MIN_HZ && c.clarity >= HIGH_CLARITY);
        // The low band's window outlives the signal by tens of milliseconds, and it cannot see a
        // fundamental above about 750 Hz, so a note cut there reads as the octave below with high
        // clarity. A signal that has just ended gets no estimate; the held one is kept.
        let pick = if self.just_cut() {
            None
        } else {
            high.or_else(|| self.low.analyse(rate))
        };
        match pick {
            Some(c) if c.hz.is_finite() && c.hz > 0.0 => {
                self.confidence = c.clarity;
                if c.clarity >= self.clarity_threshold {
                    self.estimate = c.hz;
                }
            }
            _ => self.confidence = 0.0,
        }
    }

    /// The held estimate in Hz: the last one whose confidence reached the clarity threshold, or
    /// `0.0` before the first. Always finite.
    pub fn estimate_hz(&self) -> f32 {
        self.estimate
    }

    /// The clarity of the latest analysis, in `0..1`.
    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    /// Forget the input and the estimate, keeping the parameters and sample rate.
    pub fn reset(&mut self) {
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
        self.low.reset();
        self.high.reset();
        self.since_analysis = 0;
        self.hop_energy = 0.0;
        self.hop_history = [0.0; LEVEL_HOPS];
        self.hop_head = 0;
        self.estimate = 0.0;
        self.confidence = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tracker_has_no_estimate() {
        let t = PitchTracker::new(48_000.0, &PitchParams::default());
        assert_eq!(t.estimate_hz(), 0.0);
        assert_eq!(t.confidence(), 0.0);
    }

    #[test]
    fn a_sine_is_found() {
        let mut t = PitchTracker::new(48_000.0, &PitchParams::default());
        for n in 0..24_000 {
            t.tick(0.5 * sinf(2.0 * core::f32::consts::PI * 220.0 * n as f32 / 48_000.0));
        }
        assert!((t.estimate_hz() - 220.0).abs() < 1.0, "{}", t.estimate_hz());
        assert!(t.confidence() > 0.95);
    }

    /// A decaying 880 Hz note cut hard, with or without a short fade, leaves no confident
    /// estimate at the octave below while the low band's window still holds the tail.
    #[test]
    fn a_cut_high_note_does_not_read_as_the_octave_below() {
        let rate = 48_000.0f32;
        for fade_s in [0.0f32, 0.005] {
            let cut_s = 0.44f32;
            let mut t = PitchTracker::new(
                rate,
                &PitchParams {
                    clarity_threshold: 0.8,
                },
            );
            let mut seen_note = false;
            for i in 0..(0.6 * rate) as usize {
                let time = i as f32 / rate;
                let gain = if time >= cut_s + fade_s {
                    0.0
                } else if time > cut_s {
                    1.0 - (time - cut_s) / fade_s
                } else {
                    1.0
                };
                let mut v = 0.0;
                for k in 1..=4 {
                    let kf = k as f32;
                    v += sinf(2.0 * core::f32::consts::PI * kf * 880.0 * time) / kf
                        * libm::expf(-time * kf / 0.4);
                }
                t.tick(0.4 * gain * v);
                if (i + 1) % PITCH_HOP == 0 {
                    if time < cut_s - 0.05 && t.confidence() >= 0.8 {
                        seen_note |= (t.estimate_hz() - 880.0).abs() < 10.0;
                    }
                    if time > cut_s + fade_s {
                        assert!(
                            t.confidence() < 0.8 || t.estimate_hz() > 700.0,
                            "t={time} fade={fade_s}: {} Hz at {}",
                            t.estimate_hz(),
                            t.confidence()
                        );
                    }
                }
            }
            assert!(seen_note, "the note was never found");
        }
    }
}
