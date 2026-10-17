//! Stereo gain processor with smoothed parameter transitions.

use libm::powf;

use crate::param::{ParamSpec, Taper};
use crate::processor::{Frame, Processor};
use crate::smooth::{sanitize, Smoother};

/// Parameter table for [`Gain`]; the only source of its range, taper and default.
pub const GAIN_PARAMS: [ParamSpec; 1] = [ParamSpec {
    name: "gain_db",
    unit: "dB",
    min: -60.0,
    max: 12.0,
    taper: Taper::Linear,
    default: 0.0,
}];

/// Parameters for the gain processor.
#[derive(Clone, Debug)]
pub struct GainParams {
    /// Gain in decibels. 0.0 dB is unity gain.
    pub gain_db: f32,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain_db: GAIN_PARAMS[0].default,
        }
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
        // 10^(dB/20): -60 dB = 1e-3, +6.02 dB ~ x2, +78.3 dB hits the ceiling below.
        // A non-finite gain_db falls back to unity (0 dB).
        let gain_db = if params.gain_db.is_finite() {
            params.gain_db
        } else {
            0.0
        };
        let gain_linear = powf(10.0, gain_db / 20.0);
        let gain_clamped = gain_linear.clamp(0.0, 8192.0); // ~78 dB ceiling
        self.smoother_l.set_target(gain_clamped);
        self.smoother_r.set_target(gain_clamped);
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let g_l = self.smoother_l.advance();
        let g_r = self.smoother_r.advance();
        [sanitize(input[0]) * g_l, sanitize(input[1]) * g_r]
    }

    fn reset(&mut self) {
        self.smoother_l.snap();
        self.smoother_r.snap();
    }
}

impl Gain {
    /// A gain at `sample_rate_hz` with `params` already in force, with no ramp from the default.
    pub fn new(sample_rate_hz: f32, params: &GainParams) -> Self {
        let mut g = Self::default();
        g.set_sample_rate(sample_rate_hz);
        g.set_params(params);
        g.reset();
        g
    }

    /// Jump the smoothed gain to its target now. Gain has no signal history, so this is what
    /// [`Processor::reset`] does too; it exists as its own name for the unsmoothed path.
    pub fn snap_params(&mut self) {
        self.smoother_l.snap();
        self.smoother_r.snap();
    }

    /// Convert a normalised knob position [0.0, 1.0] to [`GainParams`].
    ///
    /// Uses linear taper: maps [-60 dB silence, +12 dB headroom].
    /// At midpoint (0.5) this yields -24 dB.
    ///
    /// Input is clamped to [0.0, 1.0] so degenerate readings produce valid params.
    pub fn params_from_normalised(knob: f32) -> GainParams {
        GainParams {
            gain_db: GAIN_PARAMS[0].from_normalised(knob),
        }
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
