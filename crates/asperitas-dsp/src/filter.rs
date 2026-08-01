//! One-pole low-pass filter with smoothed coefficient transitions.

use libm::expf as exp;

use crate::processor::{Frame, Processor};
use crate::smooth::Smoother;

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
    smoother: Smoother,
    prev_l: f32,
    prev_r: f32,
}

impl Default for OnePoleLowPass {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000.0,
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
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        // Clamp cutoff to [0, sample_rate/2] to keep alpha in [0, 1].
        let nyquist = self.sample_rate_hz / 2.0;
        let cutoff = params.cutoff_hz.max(0.0).min(nyquist);
        // alpha = 1 - exp(-2π · fc / fs)
        let exponent = -(2.0 * core::f32::consts::PI) * cutoff / self.sample_rate_hz;
        let alpha = 1.0 - exp(exponent);
        self.smoother.set_target(alpha);
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let alpha = self.smoother.advance();
        self.prev_l = self.prev_l + alpha * (input[0] - self.prev_l);
        self.prev_r = self.prev_r + alpha * (input[1] - self.prev_r);
        [self.prev_l.clamp(-1.0, 1.0), self.prev_r.clamp(-1.0, 1.0)]
    }

    fn reset(&mut self) {
        self.smoother.snap();
        self.prev_l = 0.0;
        self.prev_r = 0.0;
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
