//! One-pole low-pass filter with smoothed coefficient transitions.

use crate::param::{ParamSpec, Taper};
use crate::processor::{Frame, Processor};
use crate::smooth::{sanitize, Smoother};
use libm::{expf as exp, logf as ln};

/// Parameter table for [`OnePoleLowPass`]; the only source of its range, taper and default.
pub const FILTER_PARAMS: [ParamSpec; 1] = [ParamSpec {
    name: "cutoff_hz",
    unit: "Hz",
    min: 20.0,
    max: 20_000.0,
    taper: Taper::Log,
    default: 20_000.0,
}];

/// Smoothing time constant for the log-cutoff glide, in milliseconds.
const SMOOTH_MS: f32 = 5.0;

/// Lowest cutoff the log-domain smoother can hold. A requested cutoff at or below this freezes
/// the filter state (alpha is exactly 0), because `ln(0)` is not representable.
const MIN_CUTOFF_HZ: f32 = 1e-3;

/// Headroom above `MIN_CUTOFF_HZ` inside which the filter counts as frozen. `exp(ln(MIN))` does
/// not round-trip exactly in f32, so a smoother settled on the minimum can read slightly above
/// it. Just above this ratio alpha steps from 0 to about `2π · 1e-3 / fs`.
const MIN_CUTOFF_FROZEN_RATIO: f32 = 1.001;

/// Parameters for the one-pole low-pass filter.
#[derive(Clone, Debug)]
pub struct FilterParams {
    /// Cutoff frequency in Hz. 0 Hz mutes output; values ≥ Nyquist pass through.
    pub cutoff_hz: f32,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff_hz: FILTER_PARAMS[0].default,
        }
    }
}

/// One-pole low-pass filter per channel with smoothed cutoff transitions.
///
/// Uses the standard first-order IIR: `out = prev + alpha * (in - prev)`
/// where `alpha = 1 - exp(-2π·f_c / f_s)`. The smoothed quantity is `ln(f_c)`, and alpha is
/// derived from it every sample (see the module `smooth`), so a glide is uniform in pitch.
pub struct OnePoleLowPass {
    sample_rate_hz: f32,
    /// Sanitised requested cutoff; the coefficient is derived from this and the sample rate, so
    /// the order of `set_params` and `set_sample_rate` does not matter.
    cutoff_hz: f32,
    /// Smooths `ln(cutoff_hz)`, with the cutoff already clamped to `[MIN_CUTOFF_HZ, nyquist]`.
    smoother: Smoother,
    /// The smoothed `ln(cutoff)` that `cached_alpha` was computed from; NaN when invalid, so the
    /// next tick recomputes. Bit-compared, so a settled smoother costs no `exp` calls.
    cached_ln_cutoff: f32,
    cached_alpha: f32,
    prev_l: f32,
    prev_r: f32,
}

impl Default for OnePoleLowPass {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000.0,
            cutoff_hz: 48_000.0 / 2.0,
            smoother: Smoother::new(ln(48_000.0 / 2.0), SMOOTH_MS, 48_000.0),
            cached_ln_cutoff: f32::NAN,
            cached_alpha: 0.0,
            prev_l: 0.0,
            prev_r: 0.0,
        }
    }
}

impl Processor for OnePoleLowPass {
    type Params = FilterParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 {
            self.sample_rate_hz = hz;
            self.smoother.update_sample_rate(SMOOTH_MS, hz);
            // Alpha depends on the sample rate even when ln(cutoff) does not move.
            self.cached_ln_cutoff = f32::NAN;
            self.recompute();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        // f32::max returns the non-NaN operand, so a NaN cutoff lands on 0 Hz (filter closed).
        // Gain maps NaN to unity instead; a closed low-pass is the safe failure here.
        self.cutoff_hz = params.cutoff_hz.max(0.0);
        self.recompute();
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let ln_cutoff = self.smoother.advance();
        if ln_cutoff.to_bits() != self.cached_ln_cutoff.to_bits() {
            self.cached_alpha = self.alpha(ln_cutoff);
            self.cached_ln_cutoff = ln_cutoff;
        }
        let alpha = self.cached_alpha;
        let (in_l, in_r) = (sanitize(input[0]), sanitize(input[1]));
        self.prev_l = sanitize(self.prev_l + alpha * (in_l - self.prev_l));
        self.prev_r = sanitize(self.prev_r + alpha * (in_r - self.prev_r));
        [self.prev_l, self.prev_r]
    }

    fn reset(&mut self) {
        self.smoother.snap();
        self.prev_l = 0.0;
        self.prev_r = 0.0;
    }
}

impl OnePoleLowPass {
    /// A filter at `sample_rate_hz` with `params` already in force: the first sample is filtered
    /// with the requested coefficient, not one ramped in from the default.
    pub fn new(sample_rate_hz: f32, params: &FilterParams) -> Self {
        let mut f = Self::default();
        f.set_sample_rate(sample_rate_hz);
        f.set_params(params);
        f.reset();
        f
    }

    /// Set the smoother target from the stored cutoff and sample rate. The cutoff is clamped to
    /// `[MIN_CUTOFF_HZ, nyquist]` here, so a later sample-rate change re-clamps it.
    fn recompute(&mut self) {
        let cutoff = self
            .cutoff_hz
            .min(self.sample_rate_hz / 2.0)
            .max(MIN_CUTOFF_HZ);
        self.smoother.set_target(ln(cutoff));
    }

    /// Coefficient for a smoothed `ln(cutoff_hz)`; always in `[0, 1]`.
    fn alpha(&self, ln_cutoff: f32) -> f32 {
        let cutoff = exp(ln_cutoff);
        if cutoff <= MIN_CUTOFF_HZ * MIN_CUTOFF_FROZEN_RATIO {
            return 0.0;
        }
        // alpha = 1 - exp(-2π · fc / fs)
        1.0 - exp(-(2.0 * core::f32::consts::PI) * cutoff / self.sample_rate_hz)
    }

    /// Jump the smoothed coefficient to its target now, keeping the filter state. Unlike
    /// [`Processor::reset`] this does not clear the signal history.
    pub fn snap_params(&mut self) {
        self.smoother.snap();
    }
}

#[cfg(test)]
mod normalised_tests {
    use super::*;

    fn cutoff_at(knob: f32) -> FilterParams {
        FilterParams {
            cutoff_hz: FILTER_PARAMS[0].from_normalised(knob),
        }
    }

    #[test]
    fn knob_min_is_20hz() {
        let params = cutoff_at(0.0);
        assert!((params.cutoff_hz - 20.0).abs() < 0.01);
    }

    #[test]
    fn knob_max_is_20khz() {
        let params = cutoff_at(1.0);
        assert!((params.cutoff_hz - 20_000.0).abs() < 0.01);
    }

    #[test]
    fn knob_midpoint_is_geometric_mean() {
        // Geometric mean of 20 and 20000 = sqrt(20*20000) ≈ 632.5 Hz
        let params = cutoff_at(0.5);
        assert!(
            (params.cutoff_hz - 632.5).abs() < 1.0,
            "got {}",
            params.cutoff_hz
        );
    }

    #[test]
    fn knob_clamps_below_zero() {
        let params = cutoff_at(-0.5);
        assert!((params.cutoff_hz - 20.0).abs() < 0.01);
    }

    #[test]
    fn knob_clamps_above_one() {
        let params = cutoff_at(2.0);
        assert!((params.cutoff_hz - 20_000.0).abs() < 0.01);
    }

    #[test]
    fn knob_monotonic() {
        let mut prev = 0.0_f32;
        for i in 0..=100u16 {
            let knob = i as f32 / 100.0;
            let cutoff = cutoff_at(knob).cutoff_hz;
            assert!(cutoff >= prev, "not monotonic at knob={}", knob);
            prev = cutoff;
        }
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    fn step_out(mut f: OnePoleLowPass) -> f32 {
        let mut last = 0.0;
        for _ in 0..4 {
            last = f.tick([1.0, 1.0])[0];
        }
        last
    }

    #[test]
    fn rate_and_params_commute() {
        let p = FilterParams { cutoff_hz: 1000.0 };
        let mut a = OnePoleLowPass::default();
        a.set_params(&p);
        a.set_sample_rate(8000.0);
        a.reset();
        let mut b = OnePoleLowPass::default();
        b.set_sample_rate(8000.0);
        b.set_params(&p);
        b.reset();
        assert_eq!(step_out(a).to_bits(), step_out(b).to_bits());
    }

    #[test]
    fn new_is_settled_from_sample_zero() {
        let f = OnePoleLowPass::new(8000.0, &FilterParams { cutoff_hz: 1000.0 });
        let alpha = 1.0 - exp(-2.0 * core::f32::consts::PI * 1000.0 / 8000.0);
        let mut f = f;
        assert!((f.tick([1.0, 1.0])[0] - alpha).abs() < 1e-6);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn settled(fs: f32, hz: f32) -> OnePoleLowPass {
        OnePoleLowPass::new(fs, &FilterParams { cutoff_hz: hz })
    }

    #[test]
    fn zero_cutoff_gives_alpha_exactly_zero() {
        let f = settled(48_000.0, 0.0);
        assert_eq!(f.alpha(ln(MIN_CUTOFF_HZ)).to_bits(), 0);
    }

    #[test]
    fn just_above_frozen_threshold_alpha_is_small_and_bounded() {
        let f = settled(48_000.0, 1000.0);
        let a = f.alpha(ln(MIN_CUTOFF_HZ * MIN_CUTOFF_FROZEN_RATIO * 1.01));
        let bound = 2.0 * core::f32::consts::PI * 1.1e-3 / 48_000.0;
        assert!(a > 0.0 && a <= bound, "alpha {a} bound {bound}");
    }

    #[test]
    fn cached_output_matches_uncached_over_glide_and_settle() {
        let mut cached = settled(48_000.0, 200.0);
        let mut uncached = settled(48_000.0, 200.0);
        let p = FilterParams { cutoff_hz: 5000.0 };
        cached.set_params(&p);
        uncached.set_params(&p);
        for n in 0..4000 {
            let x = [((n as f32) * 0.05).sin(), 0.3];
            uncached.cached_ln_cutoff = f32::NAN;
            let (a, b) = (cached.tick(x), uncached.tick(x));
            assert_eq!(a[0].to_bits(), b[0].to_bits(), "n={n}");
            assert_eq!(a[1].to_bits(), b[1].to_bits(), "n={n}");
        }
    }

    #[test]
    fn settled_cache_holds_and_sample_rate_change_updates_alpha() {
        let mut f = settled(48_000.0, 1000.0);
        f.tick([0.0, 0.0]);
        let a0 = f.cached_alpha;
        for _ in 0..100 {
            f.tick([0.0, 0.0]);
        }
        assert_eq!(f.cached_alpha.to_bits(), a0.to_bits());
        f.set_sample_rate(8000.0);
        f.snap_params();
        f.tick([0.0, 0.0]);
        assert!(f.cached_alpha > a0 * 3.0, "{} vs {}", f.cached_alpha, a0);
    }
}
