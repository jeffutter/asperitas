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

/// The replay effect. Default is bypass.
pub struct ReplayFx {
    kind: FxKind,
    gain: Gain,
    low_pass: OnePoleLowPass,
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
        }
    }

    /// Choose the processor and set its parameter from `knob` in `[0.0, 1.0]` (ignored for
    /// bypass). The processor starts from a clean state with no smoothing ramp, so the same
    /// selection always produces the same output.
    pub fn select(&mut self, kind: FxKind, knob: f32) {
        self.kind = kind;
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

    #[test]
    fn wire_codes() {
        assert_eq!(FxKind::from_code(0), Some(FxKind::Bypass));
        assert_eq!(FxKind::from_code(1), Some(FxKind::Gain));
        assert_eq!(FxKind::from_code(2), Some(FxKind::LowPass));
        assert_eq!(FxKind::from_code(3), None);
    }
}
