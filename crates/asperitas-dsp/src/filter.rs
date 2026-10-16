//! One-pole low-pass filter with smoothed coefficient transitions.

use libm::expf as exp;
use libm::powf;

use crate::processor::{Frame, Processor};
use crate::smooth::{sanitize, Smoother};

#[cfg(feature = "std")]
use std::string::String;

/// Parameters for the one-pole low-pass filter.
#[derive(Clone, Debug)]
pub struct FilterParams {
    /// Cutoff frequency in Hz. 0 Hz mutes output; values ≥ Nyquist pass through.
    pub cutoff_hz: f32,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff_hz: 20_000.0,
        }
    }
}

/// One-pole low-pass filter per channel with smoothed coefficient transitions.
///
/// Uses the standard first-order IIR: `out = prev + alpha * (in - prev)`
/// where `alpha = 1 - exp(-2π·f_c / f_s)`.
pub struct OnePoleLowPass {
    sample_rate_hz: f32,
    /// Sanitised requested cutoff; the coefficient is derived from this and the sample rate, so
    /// the order of `set_params` and `set_sample_rate` does not matter.
    cutoff_hz: f32,
    smoother: Smoother,
    prev_l: f32,
    prev_r: f32,
}

impl Default for OnePoleLowPass {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000.0,
            cutoff_hz: 48_000.0 / 2.0,
            smoother: Smoother::new(1.0, 5.0, 48_000.0),
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
            self.smoother.update_sample_rate(5.0, hz);
            self.recompute();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        // f32::max returns the non-NaN operand, so a NaN cutoff lands on 0 Hz.
        self.cutoff_hz = params.cutoff_hz.max(0.0);
        self.recompute();
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let alpha = self.smoother.advance();
        let (in_l, in_r) = (sanitize(input[0]), sanitize(input[1]));
        self.prev_l = sanitize(self.prev_l + alpha * (in_l - self.prev_l));
        self.prev_r = sanitize(self.prev_r + alpha * (in_r - self.prev_r));
        [self.prev_l.clamp(-1.0, 1.0), self.prev_r.clamp(-1.0, 1.0)]
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

    /// Derive the coefficient from the stored cutoff and sample rate. The cutoff is clamped to
    /// `[0, nyquist]` here, so a later sample-rate change re-clamps it, and alpha stays in
    /// `[0, 1]`.
    fn recompute(&mut self) {
        let cutoff = self.cutoff_hz.min(self.sample_rate_hz / 2.0);
        // alpha = 1 - exp(-2π · fc / fs)
        let exponent = -(2.0 * core::f32::consts::PI) * cutoff / self.sample_rate_hz;
        self.smoother.set_target(1.0 - exp(exponent));
    }

    /// Jump the smoothed coefficient to its target now, keeping the filter state. Unlike
    /// [`Processor::reset`] this does not clear the signal history.
    pub fn snap_params(&mut self) {
        self.smoother.snap();
    }

    /// Convert a normalised knob position [0.0, 1.0] to [`FilterParams`].
    ///
    /// Uses logarithmic taper: maps linearly across the frequency ratio,
    /// so equal physical knob travel gives equal perceptual steps.
    ///
    /// Range: 20 Hz (knob = 0) → 20 kHz (knob = 1).
    /// At midpoint (0.5) this yields ≈ 632 Hz (geometric mean of 20 and 20 kHz).
    ///
    /// Input is clamped to [0.0, 1.0] so degenerate readings produce valid params.
    /// NOT behind `#[cfg(feature = "std")]` — usable in no_std firmware context.
    pub fn params_from_normalised(knob: f32) -> FilterParams {
        let n = knob.clamp(0.0, 1.0);
        let freq_min = 20.0_f32;
        let freq_max = 20_000.0_f32;
        let cutoff = freq_min * powf(freq_max / freq_min, n);
        FilterParams { cutoff_hz: cutoff }
    }
}

#[cfg(feature = "std")]
impl OnePoleLowPass {
    /// Parse CLI key=value pairs into [`FilterParams`].
    ///
    /// Recognized keys: `cutoff_hz`. Returns an error for unknown keys or bad values.
    pub fn parse_params_from_cli(pairs: &[(String, String)]) -> Result<FilterParams, String> {
        let mut params = FilterParams::default();
        for (key, value) in pairs {
            match key.as_str() {
                "cutoff_hz" => {
                    params.cutoff_hz = value
                        .parse::<f32>()
                        .map_err(|_| format!("invalid value for cutoff_hz: {value}"))?;
                }
                _ => {
                    return Err(format!("unknown parameter key for filter processor: {key}"));
                }
            }
        }
        Ok(params)
    }
}

#[cfg(test)]
mod normalised_tests {
    use super::*;

    #[test]
    fn params_from_normalised_min_is_20hz() {
        let params = OnePoleLowPass::params_from_normalised(0.0);
        assert!((params.cutoff_hz - 20.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_max_is_20khz() {
        let params = OnePoleLowPass::params_from_normalised(1.0);
        assert!((params.cutoff_hz - 20_000.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_midpoint_is_geometric_mean() {
        // Geometric mean of 20 and 20000 = sqrt(20*20000) ≈ 632.5 Hz
        let params = OnePoleLowPass::params_from_normalised(0.5);
        assert!(
            (params.cutoff_hz - 632.5).abs() < 1.0,
            "got {}",
            params.cutoff_hz
        );
    }

    #[test]
    fn params_from_normalised_clamps_below_zero() {
        let params = OnePoleLowPass::params_from_normalised(-0.5);
        assert!((params.cutoff_hz - 20.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_clamps_above_one() {
        let params = OnePoleLowPass::params_from_normalised(2.0);
        assert!((params.cutoff_hz - 20_000.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_monotonic() {
        let mut prev = 0.0_f32;
        for i in 0..=100u16 {
            let knob = i as f32 / 100.0;
            let cutoff = OnePoleLowPass::params_from_normalised(knob).cutoff_hz;
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

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn filter_parse_rejects_unknown_key() {
        let err = OnePoleLowPass::parse_params_from_cli(&[("bogus_key".into(), "1000.0".into())])
            .unwrap_err();
        assert!(
            err.contains("unknown parameter"),
            "expected error about unknown parameter, got: {err}"
        );
    }

    #[test]
    fn filter_parse_rejects_non_numeric_value() {
        let err =
            OnePoleLowPass::parse_params_from_cli(&[("cutoff_hz".into(), "not_a_number".into())])
                .unwrap_err();
        assert!(
            err.contains("invalid value"),
            "expected error about invalid value, got: {err}"
        );
    }
}
