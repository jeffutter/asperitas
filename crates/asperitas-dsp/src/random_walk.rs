//! Smoothed random walk: a slowly wandering, bounded, deterministic control value.
//!
//! A modulation source, not audio, so this is not a [`crate::processor::Processor`]; it is mono
//! and a stereo user holds one per channel. It never repeats audibly and never jumps.
//!
//! # Algorithm
//!
//! The walk moves between random targets in `[-1, 1]`. A phase in `[0, 1)` advances by
//! `rate_hz / sample_rate_hz` per sample. The output is
//! `from + (to - from) * smoothstep(phase)` with `smoothstep(p) = 3p^2 - 2p^3`, which has zero
//! slope at both ends, so the output has no corners where one segment hands over to the next.
//! When the phase passes 1 the remainder is carried (never reset to 0), `from` becomes `to` and a
//! new `to` is drawn, so a segment lasts exactly `1 / rate_hz` seconds on average at any sample
//! rate.
//!
//! # Guarantees
//!
//! - Range: the output is a convex combination of two values in `[-1, 1]`, so it stays in
//!   `[-1, 1]`.
//! - Step: the largest change between consecutive samples is at most
//!   `3 * rate_hz / sample_rate_hz` ([`RandomWalk::max_step`]): smoothstep's slope is at most 1.5
//!   and two targets are at most 2 apart.
//! - Determinism: all random state is `u32` and moves with wrapping integer operations, and a
//!   draw becomes a float by an exact conversion of 24 bits, so the same seed gives the same bits
//!   on the host and on the device. The sequence does not depend on how the samples are split into
//!   blocks. [`RandomWalk::reset`] restores the exact state [`RandomWalk::new`] built.
//! - Statistics: the targets are independent and uniform, so the mean is 0 and the wander time
//!   scales with `1 / rate_hz`, independent of the sample rate.
//!
//! # PRNG
//!
//! The seed is scrambled by the MurmurHash3 32-bit finalizer (so seeds that differ by one give
//! unrelated sequences, and seed 0 is valid) and then drives xorshift32 (13, 17, 5). The crate
//! depends only on `libm`, so there is no existing generator to reuse.

use crate::param::{ParamSpec, Taper};
use crate::smooth::clean;

/// Parameter table for [`RandomWalk`]; the only source of its range, taper and default. The seed
/// is a construction input, not a parameter.
pub const RANDOM_WALK_PARAMS: [ParamSpec; 1] = [ParamSpec {
    name: "rate_hz",
    unit: "Hz",
    min: 0.01,
    max: 20.0,
    taper: Taper::Log,
    default: 0.5,
}];

/// Parameters for [`RandomWalk`].
#[derive(Clone, Debug, PartialEq)]
pub struct RandomWalkParams {
    /// New targets per second, 0.01..20. NaN falls back to the default; others clamp.
    pub rate_hz: f32,
}

impl Default for RandomWalkParams {
    fn default() -> Self {
        Self {
            rate_hz: RANDOM_WALK_PARAMS[0].default,
        }
    }
}

/// Maps the seed to a nonzero xorshift state; xorshift has a fixed point at zero.
fn scramble(seed: u32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9;
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    if h == 0 {
        0x9e37_79b9
    } else {
        h
    }
}

/// A mono smoothed random walk. See the module docs.
#[derive(Clone, Debug)]
pub struct RandomWalk {
    sample_rate_hz: f32,
    seed: u32,
    /// Requested rate, kept so `set_params` and `set_sample_rate` commute.
    rate_hz: f32,
    increment: f32,
    rng: u32,
    phase: f32,
    from: f32,
    to: f32,
    out: f32,
}

impl RandomWalk {
    /// A walk at `sample_rate_hz` with `params` in force, starting at 0 and heading for the first
    /// target drawn from `seed`.
    pub fn new(sample_rate_hz: f32, seed: u32, params: &RandomWalkParams) -> Self {
        let mut w = Self {
            sample_rate_hz: 48_000.0,
            seed,
            rate_hz: RANDOM_WALK_PARAMS[0].default,
            increment: 0.0,
            rng: 0,
            phase: 0.0,
            from: 0.0,
            to: 0.0,
            out: 0.0,
        };
        w.set_sample_rate(sample_rate_hz);
        w.set_params(params);
        w.reset();
        w
    }

    fn update_increment(&mut self) {
        // At most one segment per sample, so the phase carry below stays below 1.
        self.increment = (self.rate_hz / self.sample_rate_hz).min(1.0);
    }

    /// Retarget to a new sample rate; a rate that is not positive and finite is ignored.
    pub fn set_sample_rate(&mut self, hz: f32) {
        if hz.is_finite() && hz > 0.0 {
            self.sample_rate_hz = hz;
            self.update_increment();
        }
    }

    /// Set the wander rate. The position in the walk is kept.
    pub fn set_params(&mut self, params: &RandomWalkParams) {
        self.rate_hz = clean(params.rate_hz, &RANDOM_WALK_PARAMS[0]);
        self.update_increment();
    }

    /// The bound on the change between two consecutive samples at the current rate and sample
    /// rate: `3 * rate_hz / sample_rate_hz`.
    pub fn max_step(&self) -> f32 {
        3.0 * self.increment
    }

    fn next_target(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        // 24 bits convert to f32 exactly; this maps to [-1, 1 - 2^-23].
        (x >> 8) as f32 / 8_388_608.0 - 1.0
    }

    /// Advance by one sample and return the value, in `[-1, 1]`.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.from = self.to;
            self.to = self.next_target();
        }
        let p = self.phase.clamp(0.0, 1.0);
        let s = p * p * (3.0 - 2.0 * p);
        self.out = (self.from + (self.to - self.from) * s).clamp(-1.0, 1.0);
        self.out
    }

    /// Fill `output` with the next samples.
    pub fn process_block(&mut self, output: &mut [f32]) {
        for o in output {
            *o = self.tick();
        }
    }

    /// The most recent value; 0 before the first tick.
    pub fn value(&self) -> f32 {
        self.out
    }

    /// Return to the start of the seed's sequence, keeping the rate and sample rate.
    pub fn reset(&mut self) {
        self.rng = scramble(self.seed);
        self.phase = 0.0;
        self.from = 0.0;
        self.out = 0.0;
        self.to = self.next_target();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(rate_hz: f32) -> RandomWalkParams {
        RandomWalkParams { rate_hz }
    }

    fn run(w: &mut RandomWalk, n: usize) -> Vec<f32> {
        (0..n).map(|_| w.tick()).collect()
    }

    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    #[test]
    fn same_seed_same_sequence_different_seed_differs() {
        let mut a = RandomWalk::new(48_000.0, 7, &params(10.0));
        let mut b = RandomWalk::new(48_000.0, 7, &params(10.0));
        let mut c = RandomWalk::new(48_000.0, 8, &params(10.0));
        let (xa, xb, xc) = (
            run(&mut a, 20_000),
            run(&mut b, 20_000),
            run(&mut c, 20_000),
        );
        assert_eq!(bits(&xa), bits(&xb));
        assert_ne!(bits(&xa), bits(&xc));
    }

    #[test]
    fn seed_zero_works() {
        let mut w = RandomWalk::new(48_000.0, 0, &params(20.0));
        let x = run(&mut w, 10_000);
        assert!(x.iter().any(|v| v.abs() > 0.1));
    }

    #[test]
    fn process_block_equals_repeated_tick_for_any_split() {
        let mut a = RandomWalk::new(44_100.0, 3, &params(15.0));
        let mut b = a.clone();
        let expect = run(&mut a, 5000);
        let mut got = vec![0.0; 5000];
        let mut at = 0;
        for len in [1usize, 7, 64, 513, 1, 255, 2000, 2159] {
            b.process_block(&mut got[at..at + len]);
            at += len;
        }
        assert_eq!(at, 5000);
        assert_eq!(bits(&expect), bits(&got));
    }

    #[test]
    fn reset_equals_new_bit_for_bit() {
        let mut a = RandomWalk::new(48_000.0, 99, &params(12.0));
        run(&mut a, 7777);
        a.reset();
        assert_eq!(a.value(), 0.0);
        let mut b = RandomWalk::new(48_000.0, 99, &params(12.0));
        assert_eq!(bits(&run(&mut a, 6000)), bits(&run(&mut b, 6000)));
    }

    #[test]
    fn params_clamp_and_nan_falls_to_default() {
        let seq = |rate: f32| {
            let mut w = RandomWalk::new(48_000.0, 1, &params(rate));
            bits(&run(&mut w, 3000))
        };
        assert_eq!(seq(f32::NAN), seq(RANDOM_WALK_PARAMS[0].default));
        assert_eq!(seq(-5.0), seq(RANDOM_WALK_PARAMS[0].min));
        assert_eq!(seq(f32::NEG_INFINITY), seq(RANDOM_WALK_PARAMS[0].min));
        assert_eq!(seq(1e9), seq(RANDOM_WALK_PARAMS[0].max));
    }

    #[test]
    fn bad_sample_rate_is_ignored() {
        let mut a = RandomWalk::new(48_000.0, 1, &params(5.0));
        let mut b = a.clone();
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            a.set_sample_rate(bad);
        }
        assert_eq!(bits(&run(&mut a, 2000)), bits(&run(&mut b, 2000)));
    }

    #[test]
    fn set_params_and_set_sample_rate_commute() {
        let mut a = RandomWalk::new(48_000.0, 5, &RandomWalkParams::default());
        a.set_params(&params(9.0));
        a.set_sample_rate(8000.0);
        let mut b = RandomWalk::new(48_000.0, 5, &RandomWalkParams::default());
        b.set_sample_rate(8000.0);
        b.set_params(&params(9.0));
        assert_eq!(bits(&run(&mut a, 3000)), bits(&run(&mut b, 3000)));
    }

    #[test]
    fn extreme_rate_at_tiny_sample_rate_stays_bounded() {
        let mut w = RandomWalk::new(5.0, 2, &params(20.0));
        for v in run(&mut w, 1000) {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn step_stays_under_the_bound_at_the_fastest_rate() {
        for fs in [44_100.0, 48_000.0] {
            let mut w = RandomWalk::new(fs, 11, &params(RANDOM_WALK_PARAMS[0].max));
            let bound = w.max_step();
            let mut prev = w.value();
            for _ in 0..200_000 {
                let v = w.tick();
                assert!((v - prev).abs() <= bound * 1.0001 + 1e-7);
                prev = v;
            }
        }
    }

    /// Mean of the walk over several seeds: tolerance 0.05 against an expected spread of about
    /// 0.01 (target standard deviation 0.577 over about 8000 independent targets).
    #[test]
    fn mean_is_near_zero() {
        for fs in [44_100.0, 48_000.0] {
            let (mut sum, mut n) = (0.0f64, 0usize);
            for seed in 0..8 {
                let mut w = RandomWalk::new(fs, seed, &params(20.0));
                for v in run(&mut w, (fs as usize) * 50) {
                    sum += v as f64;
                    n += 1;
                }
            }
            let mean = sum / n as f64;
            assert!(mean.abs() < 0.05, "{fs}: mean {mean}");
        }
    }

    /// Time in seconds at which the normalised autocorrelation first falls below 1/e, measured
    /// on a series decimated by `DECIMATE` and interpolated between lags.
    fn decorrelation_s(fs: f32, rate_hz: f32) -> f32 {
        const DECIMATE: usize = 8;
        let segments = 3000.0;
        let n = (segments * fs / rate_hz) as usize;
        let mut w = RandomWalk::new(fs, 42, &params(rate_hz));
        let x: Vec<f32> = (0..n).map(|_| w.tick()).step_by(DECIMATE).collect();
        let mean = x.iter().sum::<f32>() / x.len() as f32;
        let x: Vec<f32> = x.iter().map(|v| v - mean).collect();
        let ac = |lag: usize| -> f32 {
            x[..x.len() - lag]
                .iter()
                .zip(&x[lag..])
                .map(|(a, b)| a * b)
                .sum::<f32>()
        };
        let r0 = ac(0);
        let target = 1.0 / core::f32::consts::E;
        let mut prev = 1.0;
        for lag in 1..x.len() {
            let r = ac(lag) / r0;
            if r < target {
                let frac = (prev - target) / (prev - r);
                return ((lag - 1) as f32 + frac) * DECIMATE as f32 / fs;
            }
            prev = r;
        }
        panic!("autocorrelation never fell below 1/e");
    }

    /// The wander time scales with 1 / rate_hz: doubling the rate halves it, within 10 %, and
    /// the product of rate and time is the same at 44.1 and 48 kHz within 10 %.
    #[test]
    fn wander_time_scales_with_rate_at_both_sample_rates() {
        let mut products = Vec::new();
        for fs in [44_100.0, 48_000.0] {
            let slow = decorrelation_s(fs, 10.0);
            let fast = decorrelation_s(fs, 20.0);
            let ratio = slow / fast;
            assert!((ratio - 2.0).abs() < 0.2, "{fs}: ratio {ratio}");
            products.push(slow * 10.0);
            products.push(fast * 20.0);
        }
        let lo = products.iter().cloned().fold(f32::MAX, f32::min);
        let hi = products.iter().cloned().fold(0.0, f32::max);
        assert!(hi / lo < 1.1, "rate x time varies: {products:?}");
    }
}
