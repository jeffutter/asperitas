//! Selectable effect for the rig's excerpt replay path.
//!
//! One of [`Gain`] or [`OnePoleLowPass`], chosen and parameterised from a single normalised
//! knob-style value, or bypass. Bypass touches no sample, so the replay output stays bit-exact
//! with what the DAC path produced before this existed. Parameter units follow
//! `Gain::params_from_normalised` and `OnePoleLowPass::params_from_normalised`.

use crate::filter::OnePoleLowPass;
use crate::gain::Gain;
use crate::processor::{Frame, Processor};

/// Which processor the replay runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    Bypass,
    Gain,
    LowPass,
}

impl FxKind {
    /// Map the wire code (`0` bypass, `1` gain, `2` low-pass); `None` for anything else.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(FxKind::Bypass),
            1 => Some(FxKind::Gain),
            2 => Some(FxKind::LowPass),
            _ => None,
        }
    }
}

impl FxKind {
    /// The name `describe()` reports for a non-bypass effect.
    pub fn name(self) -> &'static str {
        match self {
            FxKind::Bypass => "bypass",
            FxKind::Gain => "gain",
            FxKind::LowPass => "lowpass",
        }
    }
}

/// The replay effect. Default is bypass.
pub struct ReplayFx {
    kind: FxKind,
    gain: Gain,
    low_pass: OnePoleLowPass,
    smoothing: bool,
    /// The knob value the current processor was last parameterised from.
    knob: f32,
}

impl Default for ReplayFx {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayFx {
    /// Bypass, at 48 kHz.
    pub fn new() -> Self {
        Self {
            kind: FxKind::Bypass,
            gain: Gain::default(),
            low_pass: OnePoleLowPass::default(),
            smoothing: true,
            knob: 0.0,
        }
    }

    /// Bypass, at 48 kHz, with parameter smoothing removed: every parameter change takes effect
    /// on the next sample as a step. Intentionally click-prone, for proving that a discontinuity
    /// detector can see what smoothing normally hides. Never the shipped behaviour.
    pub fn new_unsmoothed() -> Self {
        Self {
            smoothing: false,
            ..Self::new()
        }
    }

    /// Re-parameterise the current processor from `knob` in `[0.0, 1.0]` without resetting it:
    /// filter history and any in-flight smoothing ramp carry on, so a sweep is continuous.
    /// Bypass: a no-op that touches nothing. When built unsmoothed the new value is snapped in.
    /// A `knob` equal to the one already applied does nothing, so it is cheap to call every block.
    pub fn set_knob(&mut self, knob: f32) {
        if knob == self.knob {
            return;
        }
        self.knob = knob;
        match self.kind {
            FxKind::Bypass => {}
            FxKind::Gain => {
                self.gain.set_params(&Gain::params_from_normalised(knob));
                if !self.smoothing {
                    self.gain.snap_params();
                }
            }
            FxKind::LowPass => {
                self.low_pass
                    .set_params(&OnePoleLowPass::params_from_normalised(knob));
                if !self.smoothing {
                    self.low_pass.snap_params();
                }
            }
        }
    }

    /// Choose the processor and set its parameter from `knob` in `[0.0, 1.0]` (ignored for
    /// bypass). The processor starts from a clean state with no smoothing ramp, so the same
    /// selection always produces the same output.
    pub fn select(&mut self, kind: FxKind, knob: f32) {
        self.kind = kind;
        self.knob = knob;
        match kind {
            FxKind::Bypass => {}
            FxKind::Gain => {
                self.gain.set_params(&Gain::params_from_normalised(knob));
                self.gain.reset();
            }
            FxKind::LowPass => {
                self.low_pass
                    .set_params(&OnePoleLowPass::params_from_normalised(knob));
                self.low_pass.reset();
            }
        }
    }

    /// The current selection.
    pub fn kind(&self) -> FxKind {
        self.kind
    }

    /// Process `frames` in place. No allocation; bypass returns before reading a sample.
    pub fn process(&mut self, frames: &mut [Frame]) {
        match self.kind {
            FxKind::Bypass => {}
            FxKind::Gain => {
                for f in frames {
                    *f = self.gain.tick(*f);
                }
            }
            FxKind::LowPass => {
                for f in frames {
                    *f = self.low_pass.tick(*f);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> [Frame; 32] {
        let mut b = [[0.0; 2]; 32];
        for (i, f) in b.iter_mut().enumerate() {
            let v = (i as f32 - 16.0) / 32.0;
            *f = [v, v];
        }
        b
    }

    #[test]
    fn default_is_bit_exact_bypass() {
        let mut fx = ReplayFx::new();
        let mut b = ramp();
        fx.process(&mut b);
        for (a, e) in b.iter().zip(ramp()) {
            assert_eq!(a[0].to_bits(), e[0].to_bits());
            assert_eq!(a[1].to_bits(), e[1].to_bits());
        }
    }

    #[test]
    fn gain_below_unity_attenuates() {
        // Asserts direction only: the dB-to-linear conversion is Gain's own, with its own tests.
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Gain, 0.5);
        let mut b = ramp();
        fx.process(&mut b);
        assert!(b[31][0] > 0.0 && b[31][0] < ramp()[31][0], "{}", b[31][0]);
    }

    #[test]
    fn low_pass_alters_output() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::LowPass, 0.2);
        let mut b = ramp();
        fx.process(&mut b);
        assert_ne!(b[31][0].to_bits(), ramp()[31][0].to_bits());
    }

    #[test]
    fn returning_to_bypass_restores_exact_output() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Gain, 0.1);
        fx.select(FxKind::Bypass, 0.0);
        let mut b = ramp();
        fx.process(&mut b);
        assert_eq!(b[5][0].to_bits(), ramp()[5][0].to_bits());
    }

    fn run_block(fx: &mut ReplayFx) -> [Frame; 32] {
        let mut b = [[0.1; 2]; 32];
        fx.process(&mut b);
        b
    }

    #[test]
    fn set_knob_moves_output_between_blocks() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Gain, 1.0);
        let loud = run_block(&mut fx)[31][0];
        fx.set_knob(0.2);
        for _ in 0..200 {
            run_block(&mut fx);
        }
        let quiet = run_block(&mut fx)[31][0];
        assert!(quiet < loud * 0.1, "{loud} -> {quiet}");
    }

    #[test]
    fn set_knob_on_bypass_is_bit_exact_noop() {
        let mut fx = ReplayFx::new();
        fx.set_knob(0.3);
        let mut b = ramp();
        fx.process(&mut b);
        for (a, e) in b.iter().zip(ramp()) {
            assert_eq!(a[0].to_bits(), e[0].to_bits());
        }
    }

    #[test]
    fn unsmoothed_steps_within_one_sample_smoothed_ramps() {
        let mut smooth = ReplayFx::new();
        let mut step = ReplayFx::new_unsmoothed();
        for fx in [&mut smooth, &mut step] {
            fx.select(FxKind::Gain, 1.0);
            fx.set_knob(0.5);
        }
        let target = 0.1 * 10f32.powf((-60.0 + 0.5 * 72.0) / 20.0);
        let s = run_block(&mut smooth);
        let u = run_block(&mut step);
        assert!((u[0][0] - target).abs() < 1e-6, "{} vs {target}", u[0][0]);
        assert!(
            s[0][0] > target * 2.0,
            "smoothed starts far from target: {}",
            s[0][0]
        );
        assert!(s[31][0] < s[0][0], "smoothed is still ramping");
    }

    #[test]
    fn set_knob_with_unchanged_value_leaves_ramp_alone() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Gain, 1.0);
        fx.set_knob(0.5);
        let a = run_block(&mut fx)[31][0];
        fx.set_knob(0.5);
        let b = run_block(&mut fx)[31][0];
        assert!(b < a, "ramp continues: {a} -> {b}");
    }

    #[test]
    fn set_knob_keeps_low_pass_history() {
        let mut fx = ReplayFx::new_unsmoothed();
        fx.select(FxKind::LowPass, 0.5);
        run_block(&mut fx);
        fx.set_knob(0.6);
        let b = run_block(&mut fx);
        assert!(
            b[0][0] > 0.05,
            "history kept, not restarted from zero: {}",
            b[0][0]
        );
    }

    #[test]
    fn wire_codes() {
        assert_eq!(FxKind::from_code(0), Some(FxKind::Bypass));
        assert_eq!(FxKind::from_code(1), Some(FxKind::Gain));
        assert_eq!(FxKind::from_code(2), Some(FxKind::LowPass));
        assert_eq!(FxKind::from_code(3), None);
    }
}
