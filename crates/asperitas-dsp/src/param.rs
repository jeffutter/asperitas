//! Declarative parameter descriptions shared by knob mapping, CLI parsing and tests.

use libm::{logf, powf};

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

    /// Inverse of [`from_normalised`](Self::from_normalised): the knob position that yields
    /// `value`, clamped to `[0, 1]`. NaN maps to the knob position of the default.
    pub fn to_normalised(&self, value: f32) -> f32 {
        let value = if value.is_nan() { self.default } else { value };
        let n = match self.taper {
            Taper::Linear => (value - self.min) / (self.max - self.min),
            Taper::Log => logf(value / self.min) / logf(self.max / self.min),
        };
        // A non-positive value on a log taper gives NaN or -inf; both land on the low end.
        if n.is_nan() {
            0.0
        } else {
            n.clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(taper: Taper) -> ParamSpec {
        ParamSpec {
            name: "x",
            unit: "",
            min: 20.0,
            max: 2000.0,
            taper,
            default: 200.0,
        }
    }

    #[test]
    fn to_normalised_inverts_from_normalised() {
        for taper in [Taper::Linear, Taper::Log] {
            let s = spec(taper);
            for knob in [0.0, 0.1, 0.5, 0.9, 1.0] {
                let back = s.to_normalised(s.from_normalised(knob));
                assert!((back - knob).abs() < 1e-4, "{taper:?} {knob} -> {back}");
            }
        }
    }

    #[test]
    fn to_normalised_clamps_and_handles_nan() {
        for taper in [Taper::Linear, Taper::Log] {
            let s = spec(taper);
            assert_eq!(s.to_normalised(1.0e9), 1.0);
            assert_eq!(s.to_normalised(-5.0), 0.0);
            assert_eq!(s.to_normalised(f32::NAN), s.to_normalised(s.default));
        }
    }
}
