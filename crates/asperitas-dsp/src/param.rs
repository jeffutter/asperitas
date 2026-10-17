//! Declarative parameter descriptions shared by knob mapping, CLI parsing and tests.

use libm::powf;

/// How a normalised knob position maps onto a parameter's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taper {
    /// Equal knob travel gives equal steps in the parameter value.
    Linear,
    /// Equal knob travel gives equal ratios in the parameter value. Requires `min > 0`.
    Log,
}

/// One parameter of a processor: its name, unit, range, taper and default.
///
/// The `min`/`max` range is the span a knob sweeps. It is not enforced on values supplied
/// elsewhere (CLI, tests); processors sanitise their own inputs.
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub unit: &'static str,
    pub min: f32,
    pub max: f32,
    pub taper: Taper,
    pub default: f32,
}

impl ParamSpec {
    /// Map a knob position to a value in `[min, max]`. The knob is clamped to `[0, 1]`.
    pub fn from_normalised(&self, knob: f32) -> f32 {
        let n = knob.clamp(0.0, 1.0);
        match self.taper {
            Taper::Linear => self.min + n * (self.max - self.min),
            Taper::Log => self.min * powf(self.max / self.min, n),
        }
    }
}
