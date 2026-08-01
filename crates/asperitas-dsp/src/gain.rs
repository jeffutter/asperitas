//! Stereo gain processor with smoothed parameter transitions.

use libm::expf as exp;

use crate::processor::{Frame, Processor};
use crate::smooth::Smoother;

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
