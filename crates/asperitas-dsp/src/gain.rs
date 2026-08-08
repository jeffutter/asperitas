//! Stereo gain processor with smoothed parameter transitions.

use libm::expf as exp;

use crate::processor::{Frame, Processor};
use crate::smooth::Smoother;

#[cfg(feature = "std")]
use std::string::String;

/// Parameters for the gain processor.
#[derive(Clone, Debug)]
pub struct GainParams {
    /// Gain in decibels. 0.0 dB is unity gain.
    pub gain_db: f32,
}

impl Default for GainParams {
    fn default() -> Self {
        Self { gain_db: 0.0 }
    }
}

/// Stereo gain processor with per-channel exponential smoothing.
pub struct Gain {
    sample_rate_hz: f32,
    smoother_l: Smoother,
    smoother_r: Smoother,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000.0,
            smoother_l: Smoother::new(1.0, 10.0, 48_000.0),
            smoother_r: Smoother::new(1.0, 10.0, 48_000.0),
        }
    }
}

impl Processor for Gain {
    type Params = GainParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 {
            self.sample_rate_hz = hz;
            self.smoother_l.update_sample_rate(10.0, hz);
            self.smoother_r.update_sample_rate(10.0, hz);
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        // Convert dB to linear gain, clamped to prevent overflow.
        // -96 dB ≈ 2.5e-5 (practical silence), +96 dB ≈ 4e4 (clipped by saturation).
        let gain_linear = exp(params.gain_db / 20.0);
        let gain_clamped = gain_linear.clamp(0.0, 8192.0); // ~78 dB ceiling
        self.smoother_l.set_target(gain_clamped);
        self.smoother_r.set_target(gain_clamped);
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let g_l = self.smoother_l.advance();
        let g_r = self.smoother_r.advance();
        [
            (input[0] * g_l).clamp(-1.0, 1.0),
            (input[1] * g_r).clamp(-1.0, 1.0),
        ]
    }

    fn reset(&mut self) {
        self.smoother_l.snap();
        self.smoother_r.snap();
    }
}

impl Gain {
    /// Convert a normalised knob position [0.0, 1.0] to [`GainParams`].
    ///
    /// Uses linear taper: maps [-60 dB silence, +12 dB headroom].
    /// At midpoint (0.5) this yields -24 dB.
    ///
    /// Input is clamped to [0.0, 1.0] so degenerate readings produce valid params.
    /// NOT behind `#[cfg(feature = "std")]` — usable in no_std firmware context.
    pub fn params_from_normalised(knob: f32) -> GainParams {
        let n = knob.clamp(0.0, 1.0);
        let gain_db = -60.0 + n * 72.0;
        GainParams { gain_db }
    }
}

#[cfg(feature = "std")]
impl Gain {
    /// Parse CLI key=value pairs into [`GainParams`].
    ///
    /// Recognized keys: `gain_db`. Returns an error for unknown keys or bad values.
    pub fn parse_params_from_cli(pairs: &[(String, String)]) -> Result<GainParams, String> {
        let mut params = GainParams::default();
        for (key, value) in pairs {
            match key.as_str() {
                "gain_db" => {
                    params.gain_db = value
                        .parse::<f32>()
                        .map_err(|_| format!("invalid value for gain_db: {value}"))?;
                }
                _ => {
                    return Err(format!("unknown parameter key for gain processor: {key}"));
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
    fn params_from_normalised_min_is_minus_60db() {
        let params = Gain::params_from_normalised(0.0);
        assert!((params.gain_db - (-60.0)).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_max_is_plus_12db() {
        let params = Gain::params_from_normalised(1.0);
        assert!((params.gain_db - 12.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_midpoint_is_minus_24db() {
        let params = Gain::params_from_normalised(0.5);
        assert!(
            (params.gain_db - (-24.0)).abs() < 0.01,
            "got {}",
            params.gain_db
        );
    }

    #[test]
    fn params_from_normalised_clamps_below_zero() {
        let params = Gain::params_from_normalised(-0.5);
        assert!((params.gain_db - (-60.0)).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_clamps_above_one() {
        let params = Gain::params_from_normalised(2.0);
        assert!((params.gain_db - 12.0).abs() < 0.01);
    }

    #[test]
    fn params_from_normalised_monotonic() {
        let mut prev = f32::MIN;
        for i in 0..=100u16 {
            let knob = i as f32 / 100.0;
            let gain = Gain::params_from_normalised(knob).gain_db;
            assert!(gain >= prev, "not monotonic at knob={}", knob);
            prev = gain;
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn gain_parse_rejects_unknown_key() {
        let err = Gain::parse_params_from_cli(&[("bogus_key".into(), "1.0".into())]).unwrap_err();
        assert!(
            err.contains("unknown parameter"),
            "expected error about unknown parameter, got: {err}"
        );
    }

    #[test]
    fn gain_parse_rejects_non_numeric_value() {
        let err =
            Gain::parse_params_from_cli(&[("gain_db".into(), "not_a_number".into())]).unwrap_err();
        assert!(
            err.contains("invalid value"),
            "expected error about invalid value, got: {err}"
        );
    }
}
