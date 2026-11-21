//! Envelope follower: tracks the level of a signal with separate attack and release times.
//!
//! The input is rectified and fed to a one-pole smoother whose coefficient depends on the
//! direction of travel: the attack coefficient while the rectified input is above the state, the
//! release coefficient otherwise. The output is a smooth, non-negative control signal. It is not
//! audio, so this is not a [`crate::processor::Processor`]; it is mono, and a stereo user holds
//! one follower per channel.
//!
//! Each coefficient is `1 - exp(-1000 / (ms · rate))`, which is in `[0, 1]`, so the state moves
//! toward the rectified input without overshooting it. Non-finite input samples count as silence
//! and the state is flushed through `sanitize`, so tails reach exactly `0`.
//!
//! A step of height `h` reaches `h · (1 - 1/e)` after `attack_ms` and a step down from `h` falls
//! to `h / e` after `release_ms`; both are measured in the tests below and recorded in
//! `docs/reference/resonator.md`.

use crate::param::{ParamSpec, Taper};
use crate::smooth::sanitize;
use libm::{expf as exp, fabsf};

/// Parameter table for [`EnvelopeFollower`]; the only source of its ranges, tapers and defaults.
pub const ENVELOPE_PARAMS: [ParamSpec; 2] = [
    ParamSpec {
        name: "attack_ms",
        unit: "ms",
        min: 0.1,
        max: 500.0,
        taper: Taper::Log,
        default: 5.0,
    },
    ParamSpec {
        name: "release_ms",
        unit: "ms",
        min: 1.0,
        max: 5000.0,
        taper: Taper::Log,
        default: 100.0,
    },
];

/// Parameters for [`EnvelopeFollower`].
#[derive(Clone, Debug, PartialEq)]
pub struct EnvelopeParams {
    /// Time constant while the level rises, in ms, 0.1..500. NaN falls back to the default;
    /// others clamp.
    pub attack_ms: f32,
    /// Time constant while the level falls, in ms, 1..5000. NaN falls back to the default;
    /// others clamp.
    pub release_ms: f32,
}

impl Default for EnvelopeParams {
    fn default() -> Self {
        Self {
            attack_ms: ENVELOPE_PARAMS[0].default,
            release_ms: ENVELOPE_PARAMS[1].default,
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

fn one_pole_alpha(time_ms: f32, rate: f32) -> f32 {
    1.0 - exp(-1000.0 / (time_ms * rate))
}

/// A mono attack/release level follower. See the module docs.
#[derive(Clone, Debug)]
pub struct EnvelopeFollower {
    sample_rate_hz: f32,
    /// Requested times, kept so `set_params` and `set_sample_rate` commute.
    attack_ms: f32,
    release_ms: f32,
    attack_alpha: f32,
    release_alpha: f32,
    state: f32,
}

impl EnvelopeFollower {
    /// A follower at `sample_rate_hz` with `params` in force and a zero level.
    pub fn new(sample_rate_hz: f32, params: &EnvelopeParams) -> Self {
        let mut f = Self::with_times(48_000.0, 0.0, 0.0);
        f.set_sample_rate(sample_rate_hz);
        f.set_params(params);
        f
    }

    /// A follower with raw time constants, not limited to [`ENVELOPE_PARAMS`] ranges, for stages
    /// inside this crate whose times are fixed design constants.
    pub(crate) fn with_times(sample_rate_hz: f32, attack_ms: f32, release_ms: f32) -> Self {
        let mut f = Self {
            sample_rate_hz,
            attack_ms,
            release_ms,
            attack_alpha: 0.0,
            release_alpha: 0.0,
            state: 0.0,
        };
        f.update_alphas();
        f
    }

    fn update_alphas(&mut self) {
        self.attack_alpha = one_pole_alpha(self.attack_ms, self.sample_rate_hz);
        self.release_alpha = one_pole_alpha(self.release_ms, self.sample_rate_hz);
    }

    /// Retarget to a new sample rate; a rate that is not positive and finite is ignored.
    pub fn set_sample_rate(&mut self, hz: f32) {
        if hz.is_finite() && hz > 0.0 {
            self.sample_rate_hz = hz;
            self.update_alphas();
        }
    }

    /// Set the attack and release times. The level is kept.
    pub fn set_params(&mut self, params: &EnvelopeParams) {
        self.attack_ms = clean(params.attack_ms, &ENVELOPE_PARAMS[0]);
        self.release_ms = clean(params.release_ms, &ENVELOPE_PARAMS[1]);
        self.update_alphas();
    }

    /// Advance by one sample and return the level.
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let mag = if x.is_finite() { fabsf(x) } else { 0.0 };
        let alpha = if mag > self.state {
            self.attack_alpha
        } else {
            self.release_alpha
        };
        self.state = sanitize(self.state + alpha * (mag - self.state));
        self.state
    }

    /// Follow `input` into `output`, which must be the same length.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len(), "envelope block lengths differ");
        for (o, &x) in output.iter_mut().zip(input) {
            *o = self.tick(x);
        }
    }

    /// The current level.
    pub fn value(&self) -> f32 {
        self.state
    }

    /// Return the level to zero, keeping the times and sample rate.
    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerance on a measured level at one time constant, as a fraction of the step height.
    const TOL: f32 = 0.02;
    const ONE_MINUS_INV_E: f32 = 0.632_120_56;
    const INV_E: f32 = 0.367_879_44;

    fn level_after(f: &mut EnvelopeFollower, input: f32, ms: f32, rate: f32) -> f32 {
        let n = (ms * rate / 1000.0).round() as usize;
        let mut v = 0.0;
        for _ in 0..n {
            v = f.tick(input);
        }
        v
    }

    fn params(a: f32, r: f32) -> EnvelopeParams {
        EnvelopeParams {
            attack_ms: a,
            release_ms: r,
        }
    }

    /// Step up to 1 for `attack_ms`, then step down for `release_ms`; the levels reached.
    fn measure(f: &mut EnvelopeFollower, attack_ms: f32, release_ms: f32, rate: f32) -> (f32, f32) {
        let up = level_after(f, 1.0, attack_ms, rate);
        // Settle fully high first so the fall starts from 1.
        level_after(f, 1.0, attack_ms * 20.0, rate);
        let down = level_after(f, 0.0, release_ms, rate);
        (up, down)
    }

    const SETTINGS: [(f32, f32); 4] = [(0.5, 20.0), (5.0, 100.0), (20.0, 300.0), (100.0, 2000.0)];

    #[test]
    fn time_constants_match_the_configured_values() {
        for rate in [48_000.0, 44_100.0] {
            for (a, r) in SETTINGS {
                let mut f = EnvelopeFollower::new(rate, &params(a, r));
                let (up, down) = measure(&mut f, a, r, rate);
                assert!(
                    (up - ONE_MINUS_INV_E).abs() < TOL,
                    "{rate} {a}/{r}: up {up}"
                );
                assert!((down - INV_E).abs() < TOL, "{rate} {a}/{r}: down {down}");
            }
        }
    }

    #[test]
    fn swapped_parameters_fail_the_same_measurement() {
        // Negative control: a follower with attack and release exchanged must miss the window
        // that the correct one lands in.
        for (a, r) in SETTINGS {
            let mut f = EnvelopeFollower::new(48_000.0, &params(r.min(500.0), a.max(1.0)));
            let (up, down) = measure(&mut f, a, r, 48_000.0);
            assert!(
                (up - ONE_MINUS_INV_E).abs() > TOL || (down - INV_E).abs() > TOL,
                "{a}/{r}: swapped follower passed ({up}, {down})"
            );
        }
    }

    #[test]
    fn params_clamp_and_nan_falls_to_default() {
        let mut a = EnvelopeFollower::new(48_000.0, &params(f32::NAN, f32::NAN));
        let mut b = EnvelopeFollower::new(48_000.0, &EnvelopeParams::default());
        let mut lo = EnvelopeFollower::new(48_000.0, &params(f32::NEG_INFINITY, -3.0));
        let mut lo2 = EnvelopeFollower::new(48_000.0, &params(0.1, 1.0));
        let mut hi = EnvelopeFollower::new(48_000.0, &params(f32::INFINITY, 1e9));
        let mut hi2 = EnvelopeFollower::new(48_000.0, &params(500.0, 5000.0));
        for i in 0..2000 {
            let x = if i < 1000 { 0.7 } else { 0.0 };
            assert_eq!(a.tick(x), b.tick(x));
            assert_eq!(lo.tick(x), lo2.tick(x));
            assert_eq!(hi.tick(x), hi2.tick(x));
        }
    }

    #[test]
    fn non_finite_input_counts_as_silence() {
        let mut f = EnvelopeFollower::new(48_000.0, &EnvelopeParams::default());
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(f.tick(x), 0.0);
        }
    }

    #[test]
    fn negative_input_is_rectified() {
        let mut p = EnvelopeFollower::new(48_000.0, &EnvelopeParams::default());
        let mut n = p.clone();
        for _ in 0..500 {
            assert_eq!(p.tick(0.5), n.tick(-0.5));
        }
    }

    #[test]
    fn silence_in_gives_exactly_silence_after_release() {
        let mut f = EnvelopeFollower::new(48_000.0, &params(1.0, 50.0));
        level_after(&mut f, 1.0, 100.0, 48_000.0);
        // 1e-30 is about 69 time constants of 50 ms; run well past it.
        level_after(&mut f, 0.0, 50.0 * 80.0, 48_000.0);
        assert_eq!(f.value(), 0.0);
    }

    #[test]
    fn reset_is_idempotent_and_keeps_times() {
        let mut f = EnvelopeFollower::new(48_000.0, &params(2.0, 30.0));
        level_after(&mut f, 1.0, 10.0, 48_000.0);
        f.reset();
        assert_eq!(f.value(), 0.0);
        f.reset();
        assert_eq!(f.value(), 0.0);
        let mut g = EnvelopeFollower::new(48_000.0, &params(2.0, 30.0));
        for _ in 0..300 {
            assert_eq!(f.tick(0.8), g.tick(0.8));
        }
    }

    #[test]
    fn block_equals_repeated_tick() {
        let input: Vec<f32> = (0..1000)
            .map(|i| libm::sinf(i as f32 * 0.05) * if i % 97 == 0 { f32::NAN } else { 1.0 })
            .collect();
        let mut a = EnvelopeFollower::new(48_000.0, &params(1.0, 40.0));
        let mut b = a.clone();
        let mut out = vec![0.0; input.len()];
        a.process_block(&input, &mut out);
        for (x, o) in input.iter().zip(&out) {
            assert_eq!(b.tick(*x), *o);
        }
    }

    #[test]
    fn set_params_and_set_sample_rate_commute() {
        let p = params(3.0, 60.0);
        let mut a = EnvelopeFollower::new(48_000.0, &EnvelopeParams::default());
        a.set_params(&p);
        a.set_sample_rate(8000.0);
        let mut b = EnvelopeFollower::new(48_000.0, &EnvelopeParams::default());
        b.set_sample_rate(8000.0);
        b.set_params(&p);
        for i in 0..400 {
            let x = if i < 200 { 0.6 } else { 0.0 };
            assert_eq!(a.tick(x), b.tick(x));
        }
    }
}
