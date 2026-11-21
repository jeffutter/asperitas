//! The resonator's control surface: one definition of its normalised positions, real-unit values
//! and translation into [`ResonatorParams`], [`ModulationParams`] and [`RootFollowParams`], shared
//! by every host the way `Controls` is for the low-pass and gain patch. The mapping is documented
//! in `docs/reference/resonator.md`.

use crate::envelope::{EnvelopeParams, ENVELOPE_PARAMS};
use crate::excitation::EXCITATION_PRESETS;
use crate::modulated_resonator::ModulationParams;
use crate::param::{ParamSpec, Taper};
use crate::random_walk::{RandomWalkParams, RANDOM_WALK_PARAMS};
use crate::resonator_bank::{ResonatorParams, RESONATOR_PARAMS};
use crate::root_follow::{RootFollowParams, ROOT_FOLLOW_PARAMS};
use libm::{exp2f, roundf};

/// Number of controls: root, structure, damping, spread, excitation preset, then the modulation
/// depths and rates and the root source.
pub const RESONATOR_KNOB_COUNT: usize = 13;

/// Index of the root control in [`RESONATOR_CONTROL_PARAMS`].
const ROOT: usize = 0;
/// Index of the excitation preset control in [`RESONATOR_CONTROL_PARAMS`].
const PRESET: usize = 4;
const FOLLOWER_DEPTH: usize = 5;
const WALK_DEPTH: usize = 6;
const FOLLOWER_ATTACK: usize = 7;
const FOLLOWER_RELEASE: usize = 8;
const WALK_RATE: usize = 9;
const ROOT_SOURCE: usize = 10;
const ROOT_OFFSET_SEMITONES: usize = 11;
const ROOT_OFFSET_CENTS: usize = 12;

/// The `root_source` value that follows the pitch tracker; 1 is the manual root.
const ROOT_SOURCE_TRACKED: f32 = 2.0;

/// Real-unit table for the controls, in order `[root_hz, structure, damping, spread,
/// excitation_preset, follower_depth, walk_depth, follower_attack_ms, follower_release_ms,
/// walk_rate_hz, root_source, root_offset_semitones, root_offset_cents]`. The first four are the
/// bank's own entries; the preset is the 1-based index into [`EXCITATION_PRESETS`]; the
/// modulation and offset entries copy the owning stage's table, so each range is defined once;
/// `root_source` is 1 for the manual root and 2 for the tracked one.
pub static RESONATOR_CONTROL_PARAMS: [ParamSpec; RESONATOR_KNOB_COUNT] = [
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
    ParamSpec {
        name: "follower_depth",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 0.0,
    },
    ParamSpec {
        name: "walk_depth",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 0.0,
    },
    ParamSpec {
        name: "follower_attack_ms",
        ..ENVELOPE_PARAMS[0]
    },
    ParamSpec {
        name: "follower_release_ms",
        ..ENVELOPE_PARAMS[1]
    },
    ParamSpec {
        name: "walk_rate_hz",
        ..RANDOM_WALK_PARAMS[0]
    },
    ParamSpec {
        name: "root_source",
        unit: "",
        min: 1.0,
        max: 2.0,
        taper: Taper::Linear,
        default: 1.0,
    },
    ParamSpec {
        name: "root_offset_semitones",
        ..ROOT_FOLLOW_PARAMS[0]
    },
    ParamSpec {
        name: "root_offset_cents",
        ..ROOT_FOLLOW_PARAMS[1]
    },
];

/// Whether the control at `index` takes only whole numbers (a preset, the root source, the
/// semitone offset): its position rounds to the nearest value, and automation cannot ramp it.
pub(crate) fn is_discrete(index: usize) -> bool {
    matches!(index, PRESET | ROOT_SOURCE | ROOT_OFFSET_SEMITONES)
}

/// Control surface of the resonator. Each entry is a normalised position in `[0, 1]`, mapped
/// through [`RESONATOR_CONTROL_PARAMS`]; out-of-range positions clamp and NaN falls back to the
/// parameter's default. The preset, the root source and the semitone offset are discrete: their
/// positions round to the nearest whole value. Default positions leave modulation off and the
/// root manual.
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
    /// A discrete parameter (preset, root source, semitone offset) is not a whole number.
    NotWhole { spec: ParamSpec, value: f32 },
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
            Self::NotWhole { spec, value } => write!(
                f,
                "{} must be a whole number from {} to {}, got {value}",
                spec.name, spec.min, spec.max
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
    if is_discrete(index) {
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
            if is_discrete(idx) && value != roundf(value) {
                return Err(ResonatorControlsError::NotWhole { spec, value });
            }
            controls.positions[idx] = spec.to_normalised(value);
        }
        Ok(controls)
    }

    /// The real-unit value of every control, in table order, for display. The discrete controls
    /// are whole numbers.
    pub fn values(&self) -> [f32; RESONATOR_KNOB_COUNT] {
        core::array::from_fn(|i| map_position(i, self.positions[i]))
    }

    /// The real-unit value of the control at `index` (a position in [`RESONATOR_CONTROL_PARAMS`]),
    /// translating that one control only. Panics when `index` is out of range.
    pub fn value(&self, index: usize) -> f32 {
        map_position(index, self.positions[index])
    }

    /// The resonator parameters these controls ask for.
    pub fn params(&self) -> ResonatorParams {
        // Only the first five controls: the modulation entries are not translated here, which
        // keeps the device build (the plain resonator) from paying for them.
        let at = |i: usize| map_position(i, self.positions[i]);
        let preset = (at(PRESET) as usize).clamp(1, EXCITATION_PRESETS.len()) - 1;
        ResonatorParams {
            root_hz: at(ROOT),
            structure: at(1),
            damping: at(2),
            spread: at(3),
            excitation: EXCITATION_PRESETS[preset].clone(),
        }
    }

    /// [`Self::values`], except that a control at its default position gives the table default
    /// exactly, so a log taper's round trip error never makes default controls differ from the
    /// stages' own defaults.
    fn stage_values(&self) -> [f32; RESONATOR_KNOB_COUNT] {
        core::array::from_fn(|i| {
            let spec = &RESONATOR_CONTROL_PARAMS[i];
            if self.positions[i] == spec.to_normalised(spec.default) {
                spec.default
            } else {
                map_position(i, self.positions[i])
            }
        })
    }

    /// The parameters of the modulated resonator: [`Self::params`] as the base, with the depths,
    /// the follower's attack and release and the walk's rate.
    pub fn modulation_params(&self) -> ModulationParams {
        let v = self.stage_values();
        ModulationParams {
            base: self.params(),
            follower_depth: v[FOLLOWER_DEPTH],
            walk_depth: v[WALK_DEPTH],
            follower: EnvelopeParams {
                attack_ms: v[FOLLOWER_ATTACK],
                release_ms: v[FOLLOWER_RELEASE],
            },
            walk: RandomWalkParams {
                rate_hz: v[WALK_RATE],
            },
        }
    }

    /// The parameters of the root-follow stage: tracking when the root source is tracked, with the
    /// root control as the manual root and the transposition offsets.
    pub fn root_follow_params(&self) -> RootFollowParams {
        let v = self.stage_values();
        RootFollowParams {
            tracking: v[ROOT_SOURCE] == ROOT_SOURCE_TRACKED,
            manual_root_hz: v[ROOT],
            offset_semitones: v[ROOT_OFFSET_SEMITONES],
            offset_cents: v[ROOT_OFFSET_CENTS],
        }
    }

    /// These controls with the root moved by `semitones` and `cents` (either may be negative),
    /// clamped to the root range. This is the one stepping rule for an encoder: a detent moves
    /// whole semitones, or cents while fine-tune is on.
    pub fn with_root_offset(&self, semitones: i32, cents: i32) -> Self {
        let spec = &RESONATOR_CONTROL_PARAMS[ROOT];
        let total_cents = semitones as f32 * 100.0 + cents as f32;
        let root = (self.value(ROOT) * exp2f(total_cents / 1200.0)).clamp(spec.min, spec.max);
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
        for key in [
            "follower_depth",
            "walk_depth",
            "follower_attack_ms",
            "follower_release_ms",
            "walk_rate_hz",
            "root_source",
            "root_offset_semitones",
            "root_offset_cents",
        ] {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert!(err_text(&[(key, bad)]).contains(key), "{key} {bad}");
            }
        }
        assert!(err_text(&[("follower_depth", 1.1)]).contains("follower_depth"));
        assert!(err_text(&[("walk_depth", -0.1)]).contains("walk_depth"));
        assert!(err_text(&[("follower_attack_ms", 0.05)]).contains("follower_attack_ms"));
        assert!(err_text(&[("follower_release_ms", 6000.0)]).contains("follower_release_ms"));
        assert!(err_text(&[("walk_rate_hz", 0.0)]).contains("walk_rate_hz"));
        assert!(err_text(&[("root_source", 0.0)]).contains("root_source"));
        assert!(err_text(&[("root_source", 3.0)]).contains("root_source"));
        assert!(err_text(&[("root_source", 1.5)]).contains("root_source"));
        assert!(err_text(&[("root_offset_semitones", 49.0)]).contains("root_offset_semitones"));
        assert!(err_text(&[("root_offset_semitones", 1.5)]).contains("root_offset_semitones"));
        assert!(err_text(&[("root_offset_cents", 101.0)]).contains("root_offset_cents"));
    }

    #[test]
    fn the_first_five_keys_keep_their_order_and_defaults() {
        let names: Vec<_> = RESONATOR_CONTROL_PARAMS.iter().map(|s| s.name).collect();
        assert_eq!(
            names[..5],
            [
                "root_hz",
                "structure",
                "damping",
                "spread",
                "excitation_preset"
            ]
        );
        let mut seen = names.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), RESONATOR_KNOB_COUNT, "names are unique");
        let v = ResonatorControls::default().values();
        for (value, spec) in v.iter().zip(&RESONATOR_CONTROL_PARAMS) {
            assert!((value - spec.default).abs() <= 1e-4 * spec.default.abs().max(1.0));
        }
        assert!((v[0] - 110.0).abs() < 1e-3);
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
            positions: core::array::from_fn(|i| match i {
                0 => -3.0,
                1 => 7.0,
                2 | 3 => f32::NAN,
                4 => 9.0,
                _ => 5.0,
            }),
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
        // Every new key clamps high.
        for (i, spec) in RESONATOR_CONTROL_PARAMS.iter().enumerate().skip(5) {
            assert_eq!(v[i], spec.max, "{}", spec.name);
        }
        // Every new key clamps low and falls back to its default on NaN.
        for (i, spec) in RESONATOR_CONTROL_PARAMS.iter().enumerate().skip(5) {
            let mut c = ResonatorControls::default();
            c.positions[i] = -5.0;
            assert_eq!(c.values()[i], spec.min, "{}", spec.name);
            c.positions[i] = f32::NAN;
            assert_eq!(c.values()[i], spec.default, "{}", spec.name);
        }
    }

    #[test]
    fn real_to_position_to_real_round_trips_for_every_parameter() {
        let points: [(&str, &[f32]); RESONATOR_KNOB_COUNT] = [
            ("root_hz", &[20.0, 55.0, 110.0, 440.0, 2000.0]),
            ("structure", &[0.0, 0.25, 0.5, 1.0]),
            ("damping", &[0.0, 0.3, 0.5, 1.0]),
            ("spread", &[0.0, 0.7, 1.0]),
            ("excitation_preset", &[1.0, 2.0, 3.0]),
            ("follower_depth", &[0.0, 0.15, 1.0]),
            ("walk_depth", &[0.0, 0.5, 1.0]),
            ("follower_attack_ms", &[0.1, 5.0, 500.0]),
            ("follower_release_ms", &[1.0, 200.0, 5000.0]),
            ("walk_rate_hz", &[0.01, 0.3, 20.0]),
            ("root_source", &[1.0, 2.0]),
            ("root_offset_semitones", &[-48.0, -12.0, 0.0, 7.0, 48.0]),
            ("root_offset_cents", &[-100.0, 0.0, 33.0, 100.0]),
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
    fn default_controls_leave_modulation_off_and_the_root_manual() {
        let c = ResonatorControls::default();
        assert_eq!(c.modulation_params(), ModulationParams::default());
        assert_eq!(c.root_follow_params(), RootFollowParams::default());
        assert!(!c.root_follow_params().tracking);
        assert_eq!(c.modulation_params().base, ResonatorParams::default());
    }

    #[test]
    fn modulation_and_root_follow_params_translate_the_values() {
        let c = ResonatorControls::from_values(&[
            ("root_hz", 220.0),
            ("damping", 0.3),
            ("follower_depth", 0.15),
            ("walk_depth", 0.4),
            ("follower_attack_ms", 10.0),
            ("follower_release_ms", 200.0),
            ("walk_rate_hz", 0.3),
            ("root_source", 2.0),
            ("root_offset_semitones", -12.0),
            ("root_offset_cents", 25.0),
        ])
        .unwrap();
        let m = c.modulation_params();
        assert_eq!(m.base, c.params());
        assert!((m.follower_depth - 0.15).abs() < 1e-5);
        assert!((m.walk_depth - 0.4).abs() < 1e-5);
        assert!((m.follower.attack_ms - 10.0).abs() < 1e-3);
        assert!((m.follower.release_ms - 200.0).abs() < 1e-2);
        assert!((m.walk.rate_hz - 0.3).abs() < 1e-4);
        let r = c.root_follow_params();
        assert!(r.tracking);
        assert!((r.manual_root_hz - 220.0).abs() < 1e-2);
        assert_eq!(r.offset_semitones, -12.0);
        assert!((r.offset_cents - 25.0).abs() < 1e-3);
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
