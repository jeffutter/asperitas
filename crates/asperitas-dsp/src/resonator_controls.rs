//! The resonator's control surface: one definition of its normalised positions, real-unit values
//! and translation into [`ResonatorParams`], shared by every host the way `Controls` is for the
//! low-pass and gain patch. The mapping is documented in `docs/reference/resonator.md`.

use crate::excitation::EXCITATION_PRESETS;
use crate::param::{ParamSpec, Taper};
use crate::resonator_bank::{ResonatorParams, RESONATOR_PARAMS};
use libm::{exp2f, roundf};

/// Number of controls: root, structure, damping, spread, excitation preset.
pub const RESONATOR_KNOB_COUNT: usize = 5;

/// Index of the root control in [`RESONATOR_CONTROL_PARAMS`].
const ROOT: usize = 0;
/// Index of the excitation preset control in [`RESONATOR_CONTROL_PARAMS`].
const PRESET: usize = 4;

/// Real-unit table for the controls, in order `[root_hz, structure, damping, spread,
/// excitation_preset]`. The first four are the bank's own entries; the preset is the 1-based
/// index into [`EXCITATION_PRESETS`].
pub const RESONATOR_CONTROL_PARAMS: [ParamSpec; RESONATOR_KNOB_COUNT] = [
    RESONATOR_PARAMS[0],
    RESONATOR_PARAMS[1],
    RESONATOR_PARAMS[2],
    RESONATOR_PARAMS[3],
    ParamSpec {
        name: "excitation_preset",
        unit: "",
        min: 1.0,
        max: EXCITATION_PRESETS.len() as f32,
        taper: Taper::Linear,
        default: 1.0,
    },
];

/// Control surface of the resonator. Each entry is a normalised position in `[0, 1]`, mapped
/// through [`RESONATOR_CONTROL_PARAMS`]; out-of-range positions clamp and NaN falls back to the
/// parameter's default. The preset is discrete: its position rounds to the nearest preset.
#[derive(Clone, Debug, PartialEq)]
pub struct ResonatorControls {
    pub positions: [f32; RESONATOR_KNOB_COUNT],
}

impl Default for ResonatorControls {
    fn default() -> Self {
        Self {
            positions: core::array::from_fn(|i| {
                let spec = &RESONATOR_CONTROL_PARAMS[i];
                spec.to_normalised(spec.default)
            }),
        }
    }
}

/// Why [`ResonatorControls::from_values`] rejected an input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResonatorControlsError<'a> {
    /// No entry of [`RESONATOR_CONTROL_PARAMS`] has this name.
    UnknownKey(&'a str),
    /// The value is non-finite or outside the parameter's `min..=max`.
    OutOfRange { spec: ParamSpec, value: f32 },
    /// The excitation preset is not a whole number.
    NotAPreset { value: f32 },
}

impl core::fmt::Display for ResonatorControlsError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKey(key) => write!(f, "unknown parameter key for resonator: {key}"),
            Self::OutOfRange { spec, value } => write!(
                f,
                "{} out of range for resonator: {value} (allowed {}..={} {})",
                spec.name, spec.min, spec.max, spec.unit
            ),
            Self::NotAPreset { value } => write!(
                f,
                "{} must be a whole number from 1 to {}, got {value}",
                RESONATOR_CONTROL_PARAMS[PRESET].name,
                EXCITATION_PRESETS.len()
            ),
        }
    }
}

fn map_position(index: usize, position: f32) -> f32 {
    let spec = &RESONATOR_CONTROL_PARAMS[index];
    let value = if position.is_nan() {
        spec.default
    } else {
        spec.from_normalised(position)
    };
    if index == PRESET {
        roundf(value)
    } else {
        value
    }
}

impl ResonatorControls {
    /// Build controls from real-unit `(name, value)` pairs. Parameters not named keep their table
    /// default; a repeated name takes its last value. A value outside its parameter's range is an
    /// error, never clamped.
    pub fn from_values<'a>(values: &[(&'a str, f32)]) -> Result<Self, ResonatorControlsError<'a>> {
        let mut controls = Self::default();
        for &(key, value) in values {
            let idx = RESONATOR_CONTROL_PARAMS
                .iter()
                .position(|spec| spec.name == key)
                .ok_or(ResonatorControlsError::UnknownKey(key))?;
            let spec = RESONATOR_CONTROL_PARAMS[idx];
            if !value.is_finite() || value < spec.min || value > spec.max {
                return Err(ResonatorControlsError::OutOfRange { spec, value });
            }
            if idx == PRESET && value != roundf(value) {
                return Err(ResonatorControlsError::NotAPreset { value });
            }
            controls.positions[idx] = spec.to_normalised(value);
        }
        Ok(controls)
    }

    /// The real-unit value of every control, in table order, for display. The preset is a whole
    /// number.
    pub fn values(&self) -> [f32; RESONATOR_KNOB_COUNT] {
        core::array::from_fn(|i| map_position(i, self.positions[i]))
    }

    /// The resonator parameters these controls ask for.
    pub fn params(&self) -> ResonatorParams {
        let v = self.values();
        let preset = (v[PRESET] as usize).clamp(1, EXCITATION_PRESETS.len()) - 1;
        ResonatorParams {
            root_hz: v[ROOT],
            structure: v[1],
            damping: v[2],
            spread: v[3],
            excitation: EXCITATION_PRESETS[preset].clone(),
        }
    }

    /// These controls with the root moved by `semitones` and `cents` (either may be negative),
    /// clamped to the root range. This is the one stepping rule for an encoder: a detent moves
    /// whole semitones, or cents while fine-tune is on.
    pub fn with_root_offset(&self, semitones: i32, cents: i32) -> Self {
        let spec = &RESONATOR_CONTROL_PARAMS[ROOT];
        let total_cents = semitones as f32 * 100.0 + cents as f32;
        let root = (self.values()[ROOT] * exp2f(total_cents / 1200.0)).clamp(spec.min, spec.max);
        let mut next = self.clone();
        next.positions[ROOT] = spec.to_normalised(root);
        next
    }
}

impl From<&ResonatorControls> for ResonatorParams {
    fn from(controls: &ResonatorControls) -> Self {
        controls.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_text(values: &[(&str, f32)]) -> String {
        ResonatorControls::from_values(values)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn from_values_refuses_each_bad_input_naming_the_parameter() {
        assert!(err_text(&[("bogus", 1.0)]).contains("bogus"));
        for key in ["root_hz", "structure", "damping", "spread"] {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert!(err_text(&[(key, bad)]).contains(key), "{key} {bad}");
            }
        }
        assert!(err_text(&[("root_hz", 19.0)]).contains("root_hz"));
        assert!(err_text(&[("root_hz", 2001.0)]).contains("root_hz"));
        assert!(err_text(&[("structure", -0.1)]).contains("structure"));
        assert!(err_text(&[("damping", 1.1)]).contains("damping"));
        assert!(err_text(&[("spread", 2.0)]).contains("spread"));
        assert!(err_text(&[("excitation_preset", 0.0)]).contains("excitation_preset"));
        assert!(err_text(&[("excitation_preset", 4.0)]).contains("excitation_preset"));
        assert!(err_text(&[("excitation_preset", 1.5)]).contains("excitation_preset"));
    }

    #[test]
    fn unnamed_keys_keep_defaults_and_the_last_value_wins() {
        let c = ResonatorControls::from_values(&[("damping", 0.2), ("damping", 0.8)]).unwrap();
        let v = c.values();
        assert!((v[2] - 0.8).abs() < 1e-5);
        assert!((v[0] - 110.0).abs() < 1e-3);
        assert_eq!(
            ResonatorControls::from_values(&[]).unwrap(),
            ResonatorControls::default()
        );
    }

    #[test]
    fn positions_clamp_and_nan_falls_back_to_the_default() {
        let mut c = ResonatorControls {
            positions: [-3.0, 7.0, f32::NAN, f32::NAN, 9.0],
        };
        let v = c.values();
        assert_eq!(v[0], 20.0);
        assert_eq!(v[1], 1.0);
        assert_eq!(v[2], 0.5);
        assert_eq!(v[3], 1.0);
        assert_eq!(v[4], 3.0);
        let p = c.params();
        assert_eq!(p.damping, 0.5);
        assert_eq!(p.spread, 1.0);
        assert_eq!(p.excitation, EXCITATION_PRESETS[2]);
        c.positions[4] = f32::NAN;
        assert_eq!(c.params().excitation, EXCITATION_PRESETS[0]);
    }

    #[test]
    fn real_to_position_to_real_round_trips_for_every_parameter() {
        let points: [(&str, &[f32]); 5] = [
            ("root_hz", &[20.0, 55.0, 110.0, 440.0, 2000.0]),
            ("structure", &[0.0, 0.25, 0.5, 1.0]),
            ("damping", &[0.0, 0.3, 0.5, 1.0]),
            ("spread", &[0.0, 0.7, 1.0]),
            ("excitation_preset", &[1.0, 2.0, 3.0]),
        ];
        for (idx, (key, vals)) in points.iter().enumerate() {
            for &want in *vals {
                let c = ResonatorControls::from_values(&[(key, want)]).unwrap();
                let got = c.values()[idx];
                assert!(
                    (got - want).abs() <= 1e-4 * want.abs().max(1.0),
                    "{key}: {want} -> {got}"
                );
            }
        }
    }

    #[test]
    fn default_controls_give_the_default_params() {
        assert_eq!(
            ResonatorControls::default().params(),
            ResonatorParams::default()
        );
        assert_eq!(
            ResonatorParams::from(&ResonatorControls::default()),
            ResonatorParams::default()
        );
    }

    #[test]
    fn knobs_one_and_two_are_structure_and_damping_and_the_preset_cycles() {
        let mut c = ResonatorControls::default();
        c.positions[1] = 0.25;
        c.positions[2] = 0.75;
        let p = c.params();
        assert!((p.structure - 0.25).abs() < 1e-6);
        assert!((p.damping - 0.75).abs() < 1e-6);
        assert_eq!(p.root_hz, 110.0);
        for (i, preset) in EXCITATION_PRESETS.iter().enumerate() {
            let c =
                ResonatorControls::from_values(&[("excitation_preset", (i + 1) as f32)]).unwrap();
            assert_eq!(&c.params().excitation, preset);
        }
    }

    #[test]
    fn root_offset_steps_in_semitones_and_cents_and_clamps() {
        let c = ResonatorControls::from_values(&[("root_hz", 110.0)]).unwrap();
        let up = c.with_root_offset(12, 0).values()[0];
        assert!((up - 220.0).abs() < 0.01, "{up}");
        let down = c.with_root_offset(-12, 0).values()[0];
        assert!((down - 55.0).abs() < 0.01, "{down}");
        let cents = c.with_root_offset(0, 100).values()[0];
        assert!((cents - 110.0 * exp2f(1.0 / 12.0)).abs() < 0.01, "{cents}");
        assert_eq!(c.with_root_offset(1000, 0).values()[0], 2000.0);
        assert_eq!(c.with_root_offset(-1000, 0).values()[0], 20.0);
        assert_eq!(c.with_root_offset(0, 0), c);
    }
}
