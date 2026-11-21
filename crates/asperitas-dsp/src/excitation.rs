//! Excitation shaping: conditions an instrument signal before it drives a resonator bank.
//!
//! Per channel, three stages in series:
//!
//! 1. A one-pole high-pass (the low cut) removes the low body that would make a bank boom.
//! 2. A transient emphasis multiplies the signal by `1 + 2 · amount · attack`, where `attack` in
//!    `[0, 1)` compares a fast and a slow envelope follower of the signal. It is a ratio, so the
//!    emphasis does not depend on the signal's level, and it is exactly `0` in steady state or a
//!    decay.
//! 3. A soft limiter is the identity up to half its ceiling and approaches the ceiling
//!    asymptotically above that. It is last, so the output is bounded by the ceiling for any
//!    bounded input whatever the other stages do.
//!
//! The channels are processed independently with identical coefficients, so a mono signal on
//! either or both sides behaves the same. The defaults are transparent: a 10 Hz low cut, no
//! emphasis and a ceiling of 2.0 (a linear region up to 1.0, full scale).

use crate::envelope::EnvelopeFollower;
use crate::param::{ParamSpec, Taper};
use crate::processor::{Frame, Processor};
use crate::smooth::{sanitize, Smoother};
use libm::{copysignf, expf as exp, fabsf, logf as ln, tanhf};

/// Parameter table for [`Excitation`]; the only source of its ranges, tapers and defaults.
pub const EXCITATION_PARAMS: [ParamSpec; 3] = [
    ParamSpec {
        name: "low_cut_hz",
        unit: "Hz",
        min: 10.0,
        max: 1000.0,
        taper: Taper::Log,
        default: 10.0,
    },
    ParamSpec {
        name: "transient",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 0.0,
    },
    ParamSpec {
        name: "drive_limit",
        unit: "",
        min: 0.25,
        max: 2.0,
        taper: Taper::Linear,
        default: 2.0,
    },
];

/// Three excitation voicings, from the one that leaves the signal alone to the one that shapes
/// it hardest. The first equals [`ExcitationParams::default`].
pub const EXCITATION_PRESETS: [ExcitationParams; 3] = [
    ExcitationParams {
        low_cut_hz: 10.0,
        transient: 0.0,
        drive_limit: 2.0,
    },
    ExcitationParams {
        low_cut_hz: 80.0,
        transient: 0.3,
        drive_limit: 1.0,
    },
    ExcitationParams {
        low_cut_hz: 200.0,
        transient: 0.6,
        drive_limit: 0.5,
    },
];

/// Smoothing time constant for `ln(low_cut_hz)`, in milliseconds.
const CUT_SMOOTH_MS: f32 = 5.0;
/// Smoothing time constant for the transient amount, in milliseconds.
const TRANSIENT_SMOOTH_MS: f32 = 5.0;
/// Smoothing time constant for the ceiling, in milliseconds.
const LEVEL_SMOOTH_MS: f32 = 10.0;
/// Time constant of the fast envelope follower, in milliseconds.
const FAST_ENV_MS: f32 = 1.0;
/// Time constant of the slow envelope follower, in milliseconds.
const SLOW_ENV_MS: f32 = 40.0;
/// Added to the fast envelope in the attack ratio so silence does not divide by zero and noise
/// floor ripple (about -80 dB) does not read as an attack.
const ATTACK_FLOOR: f32 = 1e-4;
/// Largest multiplier the transient stage can apply (at `transient = 1`, ratio -> 1).
const MAX_TRANSIENT_GAIN: f32 = 3.0;

/// Parameters for the excitation stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ExcitationParams {
    /// High-pass corner in Hz, 10..1000. NaN falls back to the default; others clamp.
    pub low_cut_hz: f32,
    /// Attack emphasis, 0..1. 0 bypasses the stage exactly.
    pub transient: f32,
    /// Limiter ceiling as an amplitude, 0.25..2.0. Output never exceeds it.
    pub drive_limit: f32,
}

impl Default for ExcitationParams {
    fn default() -> Self {
        Self {
            low_cut_hz: EXCITATION_PARAMS[0].default,
            transient: EXCITATION_PARAMS[1].default,
            drive_limit: EXCITATION_PARAMS[2].default,
        }
    }
}

/// NaN becomes the spec's default and everything else clamps to its range.
fn clean(v: f32, spec: &ParamSpec) -> f32 {
    if v.is_nan() {
        spec.default
    } else {
        v.clamp(spec.min, spec.max)
    }
}

/// Identity up to `ceiling / 2`, then a tanh knee that is continuous in value and slope and
/// approaches `ceiling` from below.
fn soft_limit(x: f32, ceiling: f32) -> f32 {
    let knee = 0.5 * ceiling;
    let a = fabsf(x);
    if a <= knee {
        return x;
    }
    let room = ceiling - knee;
    copysignf(knee + room * tanhf((a - knee) / room), x)
}

/// Per-channel state.
#[derive(Clone)]
struct Channel {
    /// One-pole low-pass of the input; the high-pass output is the input minus this.
    lp: f32,
    fast: EnvelopeFollower,
    slow: EnvelopeFollower,
}

impl Channel {
    fn new(sample_rate_hz: f32) -> Self {
        Self {
            lp: 0.0,
            fast: EnvelopeFollower::with_times(sample_rate_hz, FAST_ENV_MS, FAST_ENV_MS),
            slow: EnvelopeFollower::with_times(sample_rate_hz, SLOW_ENV_MS, SLOW_ENV_MS),
        }
    }
}

/// The excitation stage. See the module docs for the signal flow.
pub struct Excitation {
    sample_rate_hz: f32,
    /// Sanitised requested low cut; the coefficient derives from this and the sample rate, so
    /// `set_params` and `set_sample_rate` commute.
    cut_hz: f32,
    cut: Smoother,
    amount: Smoother,
    ceiling: Smoother,
    /// The smoothed `ln(cut)` that `cached_alpha` came from; NaN when stale.
    cached_ln_cut: f32,
    cached_alpha: f32,
    ch: [Channel; 2],
}

impl Default for Excitation {
    fn default() -> Self {
        let p = ExcitationParams::default();
        Self {
            sample_rate_hz: 48_000.0,
            cut_hz: p.low_cut_hz,
            cut: Smoother::new(ln(p.low_cut_hz), CUT_SMOOTH_MS, 48_000.0),
            amount: Smoother::new(p.transient, TRANSIENT_SMOOTH_MS, 48_000.0),
            ceiling: Smoother::new(p.drive_limit, LEVEL_SMOOTH_MS, 48_000.0),
            cached_ln_cut: f32::NAN,
            cached_alpha: 0.0,
            ch: [Channel::new(48_000.0), Channel::new(48_000.0)],
        }
    }
}

impl Excitation {
    /// A stage at `sample_rate_hz` with `params` already in force, with no ramp from the default.
    pub fn new(sample_rate_hz: f32, params: &ExcitationParams) -> Self {
        let mut e = Self::default();
        e.set_sample_rate(sample_rate_hz);
        e.set_params(params);
        e.reset();
        e
    }

    /// Jump the smoothed parameters to their targets now, keeping the signal state.
    pub fn snap_params(&mut self) {
        self.cut.snap();
        self.amount.snap();
        self.ceiling.snap();
    }

    fn retarget_cut(&mut self) {
        let hi = (self.sample_rate_hz / 2.0).max(EXCITATION_PARAMS[0].min);
        self.cut.set_target(ln(self.cut_hz.min(hi)));
    }

    fn alpha(&self, ln_cut: f32) -> f32 {
        1.0 - exp(-(2.0 * core::f32::consts::PI) * exp(ln_cut) / self.sample_rate_hz)
    }

    #[inline]
    fn shape(&mut self, c: usize, x: f32, alpha: f32, amount: f32, ceiling: f32) -> f32 {
        let s = &mut self.ch[c];
        s.lp = sanitize(s.lp + alpha * (x - s.lp));
        let hp = x - s.lp;
        let fast = s.fast.tick(hp);
        let slow = s.slow.tick(hp);
        let shaped = if amount > 0.0 {
            let attack = (fast - slow).max(0.0) / (fast + ATTACK_FLOOR);
            hp * (1.0 + (MAX_TRANSIENT_GAIN - 1.0) * amount * attack)
        } else {
            hp
        };
        soft_limit(shaped, ceiling)
    }
}

impl Processor for Excitation {
    type Params = ExcitationParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 {
            self.sample_rate_hz = hz;
            self.cut.update_sample_rate(CUT_SMOOTH_MS, hz);
            self.amount.update_sample_rate(TRANSIENT_SMOOTH_MS, hz);
            self.ceiling.update_sample_rate(LEVEL_SMOOTH_MS, hz);
            for c in &mut self.ch {
                c.fast.set_sample_rate(hz);
                c.slow.set_sample_rate(hz);
            }
            self.cached_ln_cut = f32::NAN;
            self.retarget_cut();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.cut_hz = clean(params.low_cut_hz, &EXCITATION_PARAMS[0]);
        self.retarget_cut();
        self.amount
            .set_target(clean(params.transient, &EXCITATION_PARAMS[1]));
        self.ceiling
            .set_target(clean(params.drive_limit, &EXCITATION_PARAMS[2]));
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let ln_cut = self.cut.advance();
        if ln_cut.to_bits() != self.cached_ln_cut.to_bits() {
            self.cached_alpha = self.alpha(ln_cut);
            self.cached_ln_cut = ln_cut;
        }
        let (alpha, amount, ceiling) = (
            self.cached_alpha,
            self.amount.advance(),
            self.ceiling.advance(),
        );
        [
            self.shape(0, sanitize(input[0]), alpha, amount, ceiling),
            self.shape(1, sanitize(input[1]), alpha, amount, ceiling),
        ]
    }

    fn reset(&mut self) {
        self.snap_params();
        self.ch = [
            Channel::new(self.sample_rate_hz),
            Channel::new(self.sample_rate_hz),
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    const FS: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<Frame> {
        (0..n)
            .map(|i| {
                let v = amp * libm::sinf(2.0 * PI * hz * i as f32 / FS);
                [v, v]
            })
            .collect()
    }

    fn run(params: &ExcitationParams, input: &[Frame]) -> Vec<Frame> {
        let mut e = Excitation::new(FS, params);
        let mut out = input.to_vec();
        e.process_block(&mut out);
        out
    }

    /// Gain in dB of the last half of a rendered sine, by RMS ratio.
    fn gain_db(params: &ExcitationParams, hz: f32, amp: f32) -> f32 {
        let n = 24_000;
        let input = sine(hz, amp, n);
        let out = run(params, &input);
        let rms = |b: &[Frame]| {
            let s: f32 = b[n / 2..].iter().map(|f| f[0] * f[0]).sum();
            libm::sqrtf(s / (n / 2) as f32)
        };
        20.0 * libm::log10f(rms(&out) / rms(&input))
    }

    fn peak(b: &[Frame]) -> f32 {
        b.iter().fold(0.0, |m, f| m.max(f[0].abs()))
    }

    #[test]
    fn default_is_transparent_across_the_instrument_range() {
        let mut worst = 0.0f32;
        for hz in [41.0, 82.0, 165.0, 330.0, 660.0, 1320.0, 2640.0] {
            let g = gain_db(&ExcitationParams::default(), hz, 0.25);
            worst = worst.max(g.abs());
            assert!(g.abs() < 0.5, "{hz} Hz: {g} dB");
        }
        // The worst case is the 41 Hz low E: a 10 Hz one-pole is about -0.24 dB there.
        assert!(worst > 0.1 && worst < 0.5, "worst {worst}");
    }

    #[test]
    fn default_limiter_is_linear_through_full_scale() {
        for x in [0.0, 0.1, -0.7, 1.0, -1.0] {
            assert_eq!(soft_limit(x, 2.0), x);
        }
    }

    #[test]
    fn raising_low_cut_lowers_low_frequencies_only() {
        let base = ExcitationParams::default();
        let cut = ExcitationParams {
            low_cut_hz: 200.0,
            ..base.clone()
        };
        let (low0, low1) = (gain_db(&base, 55.0, 0.25), gain_db(&cut, 55.0, 0.25));
        let (hi0, hi1) = (gain_db(&base, 2000.0, 0.25), gain_db(&cut, 2000.0, 0.25));
        // Measured: 55 Hz falls by about 12 dB; 2 kHz moves by -0.15 dB (the one-pole high-pass).
        assert!(low1 < low0 - 10.0, "55 Hz {low0} -> {low1} dB");
        assert!((hi1 - hi0).abs() < 0.25, "2 kHz {hi0} -> {hi1} dB");
    }

    #[test]
    fn raising_transient_emphasises_the_attack_not_the_sustain() {
        // A 220 Hz note that starts after silence, decaying with a 100 ms time constant.
        let n = 12_000;
        let mut note = vec![[0.0f32; 2]; 480];
        note.extend((0..n).map(|i| {
            let t = i as f32 / FS;
            let v = 0.25 * libm::expf(-t / 0.1) * libm::sinf(2.0 * PI * 220.0 * t);
            [v, v]
        }));
        let off = ExcitationParams::default();
        let on = ExcitationParams {
            transient: 1.0,
            ..off.clone()
        };
        let (a, b) = (run(&off, &note), run(&on, &note));
        // The first 10 ms after onset, then the tail from 150 ms on.
        let attack = |o: &[Frame]| peak(&o[480..480 + 480]);
        let tail = |o: &[Frame]| peak(&o[480 + 7200..]);
        let attack_gain_db = 20.0 * libm::log10f(attack(&b) / attack(&a));
        let tail_gain_db = 20.0 * libm::log10f(tail(&b) / tail(&a));
        assert!(attack_gain_db > 3.0, "attack gain {attack_gain_db} dB");
        assert!(tail_gain_db.abs() < 1.0, "tail gain {tail_gain_db} dB");
    }

    #[test]
    fn lowering_drive_limit_lowers_a_loud_sine_toward_the_ceiling() {
        let loud = sine(330.0, 1.0, 9600);
        let mut last = f32::MAX;
        for ceiling in [2.0, 1.0, 0.5, 0.25] {
            let p = ExcitationParams {
                drive_limit: ceiling,
                ..ExcitationParams::default()
            };
            let pk = peak(&run(&p, &loud));
            assert!(pk < last && pk <= ceiling, "ceiling {ceiling}: peak {pk}");
            last = pk;
        }
        // At 0.25 the 1.0 input is far past the knee, so the peak sits just under the ceiling.
        assert!(last > 0.22, "peak {last}");
    }

    #[test]
    fn presets_start_at_default_and_stay_in_range() {
        assert_eq!(EXCITATION_PRESETS[0], ExcitationParams::default());
        for p in &EXCITATION_PRESETS {
            for (v, s) in [
                (p.low_cut_hz, &EXCITATION_PARAMS[0]),
                (p.transient, &EXCITATION_PARAMS[1]),
                (p.drive_limit, &EXCITATION_PARAMS[2]),
            ] {
                assert!((s.min..=s.max).contains(&v), "{} = {v}", s.name);
            }
        }
    }

    #[test]
    fn transient_zero_is_a_bit_exact_bypass_of_that_stage() {
        let off = ExcitationParams::default();
        let tiny = ExcitationParams {
            transient: 0.0,
            ..off.clone()
        };
        let x = sine(330.0, 0.5, 2000);
        assert_eq!(run(&off, &x), run(&tiny, &x));
    }

    #[test]
    fn nan_params_fall_to_default_and_infinities_clamp() {
        let nan = ExcitationParams {
            low_cut_hz: f32::NAN,
            transient: f32::NAN,
            drive_limit: f32::NAN,
        };
        let x = sine(330.0, 0.5, 2000);
        assert_eq!(run(&nan, &x), run(&ExcitationParams::default(), &x));
        let inf = ExcitationParams {
            low_cut_hz: f32::INFINITY,
            transient: f32::INFINITY,
            drive_limit: f32::NEG_INFINITY,
        };
        assert!(peak(&run(&inf, &x)) <= 0.25);
    }

    #[test]
    fn rate_and_params_commute() {
        let p = EXCITATION_PRESETS[2].clone();
        let x = sine(330.0, 0.5, 500);
        let mut a = Excitation::default();
        a.set_params(&p);
        a.set_sample_rate(8000.0);
        a.reset();
        let mut b = Excitation::default();
        b.set_sample_rate(8000.0);
        b.set_params(&p);
        b.reset();
        for f in &x {
            assert_eq!(a.tick(*f), b.tick(*f));
        }
    }

    #[test]
    fn a_step_in_every_parameter_stays_below_the_click_threshold() {
        // Step all three at once while a 997 Hz sine at 0.5 plays; the largest sample-to-sample
        // change must stay under 0.1, against 0.065 for the steady sine itself.
        let x = sine(997.0, 0.5, 9600);
        let mut e = Excitation::new(FS, &ExcitationParams::default());
        let mut out = x.clone();
        e.process_block(&mut out[..4800]);
        e.set_params(&EXCITATION_PRESETS[2]);
        e.process_block(&mut out[4800..]);
        let worst = out
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.1, "max step {worst}");
    }
}
