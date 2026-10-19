//! The top-level Asperitas effect: low-pass filter then gain, driven by normalised knobs.

use crate::filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
use crate::gain::{Gain, GainParams, GAIN_PARAMS};
use crate::param::ParamSpec;
use crate::processor::{Frame, Processor};

/// Number of knobs: filter cutoff, then output gain.
pub const KNOB_COUNT: usize = 2;

/// Real-unit parameter table for the knobs, in knob order `[cutoff, gain]`.
pub const ASPERITAS_PARAMS: [ParamSpec; KNOB_COUNT] = [FILTER_PARAMS[0], GAIN_PARAMS[0]];

/// Control surface of the effect. Each knob is a normalised position in `[0, 1]`; out-of-range
/// values clamp and NaN falls back to the parameter's default.
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    /// `[cutoff, gain]`, mapped through [`FILTER_PARAMS`] and [`GAIN_PARAMS`].
    pub knobs: [f32; KNOB_COUNT],
}

impl Default for Controls {
    /// Knob positions that give every parameter its table default (filter open, 0 dB).
    fn default() -> Self {
        Self {
            knobs: [
                FILTER_PARAMS[0].to_normalised(FILTER_PARAMS[0].default),
                GAIN_PARAMS[0].to_normalised(GAIN_PARAMS[0].default),
            ],
        }
    }
}

/// Why [`Controls::from_values`] rejected an input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlsError<'a> {
    /// No entry of [`ASPERITAS_PARAMS`] has this name.
    UnknownKey(&'a str),
    /// The value is non-finite or outside the parameter's `min..=max`.
    OutOfRange { spec: ParamSpec, value: f32 },
}

impl core::fmt::Display for ControlsError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKey(key) => {
                write!(f, "unknown parameter key for asperitas processor: {key}")
            }
            Self::OutOfRange { spec, value } => write!(
                f,
                "{} out of range for asperitas processor: {value} (allowed {}..={} {})",
                spec.name, spec.min, spec.max, spec.unit
            ),
        }
    }
}

impl Controls {
    /// Build controls from real-unit `(name, value)` pairs, the one translation from real units
    /// to knob positions. Parameters not named keep their table default; a repeated name takes
    /// its last value. Unlike a knob reading, which clamps, a value outside its parameter's range
    /// is an error rather than silently moved.
    pub fn from_values<'a>(values: &[(&'a str, f32)]) -> Result<Self, ControlsError<'a>> {
        let mut controls = Self::default();
        for &(key, value) in values {
            let idx = ASPERITAS_PARAMS
                .iter()
                .position(|spec| spec.name == key)
                .ok_or(ControlsError::UnknownKey(key))?;
            let spec = ASPERITAS_PARAMS[idx];
            if !value.is_finite() || value < spec.min || value > spec.max {
                return Err(ControlsError::OutOfRange { spec, value });
            }
            controls.knobs[idx] = spec.to_normalised(value);
        }
        Ok(controls)
    }

    /// The real-unit value of every knob, in knob order, for display.
    pub fn values(&self) -> [f32; KNOB_COUNT] {
        core::array::from_fn(|i| map_knob(&ASPERITAS_PARAMS[i], self.knobs[i]))
    }
}

fn map_knob(spec: &ParamSpec, knob: f32) -> f32 {
    if knob.is_nan() {
        spec.default
    } else {
        spec.from_normalised(knob)
    }
}

fn filter_params(controls: &Controls) -> FilterParams {
    FilterParams {
        cutoff_hz: map_knob(&FILTER_PARAMS[0], controls.knobs[0]),
    }
}

fn gain_params(controls: &Controls) -> GainParams {
    GainParams {
        gain_db: map_knob(&GAIN_PARAMS[0], controls.knobs[1]),
    }
}

/// Filter followed by gain, processed in place. The only clipping is the hard clamp at the
/// codec boundary, outside this type.
pub struct Asperitas {
    filter: OnePoleLowPass,
    gain: Gain,
}

impl Asperitas {
    /// An effect at `sample_rate_hz` with `controls` already in force, with no ramp from defaults.
    pub fn new(sample_rate_hz: f32, controls: &Controls) -> Self {
        Self {
            filter: OnePoleLowPass::new(sample_rate_hz, &filter_params(controls)),
            gain: Gain::new(sample_rate_hz, &gain_params(controls)),
        }
    }

    /// Jump both stages to their current targets with no ramp. For proving that a discontinuity
    /// detector can see what smoothing normally hides; never the shipped behaviour.
    pub fn snap_params(&mut self) {
        self.filter.snap_params();
        self.gain.snap_params();
    }

    /// Apply knob positions; the stages smooth the transition.
    pub fn set_controls(&mut self, controls: &Controls) {
        self.filter.set_params(&filter_params(controls));
        self.gain.set_params(&gain_params(controls));
    }
}

impl Default for Asperitas {
    fn default() -> Self {
        Self::new(48_000.0, &Controls::default())
    }
}

impl Processor for Asperitas {
    type Params = Controls;

    fn set_sample_rate(&mut self, hz: f32) {
        self.filter.set_sample_rate(hz);
        self.gain.set_sample_rate(hz);
    }

    fn set_params(&mut self, params: &Controls) {
        self.set_controls(params);
    }

    fn tick(&mut self, input: Frame) -> Frame {
        self.gain.tick(self.filter.tick(input))
    }

    fn reset(&mut self) {
        self.filter.reset();
        self.gain.reset();
    }

    fn process_block(&mut self, block: &mut [Frame]) {
        self.filter.process_block(block);
        self.gain.process_block(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAIRS: [[f32; 2]; 4] = [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0], [0.2, 0.9]];

    fn input(n: usize) -> Vec<Frame> {
        let mut s = 0x1234_5678_u32;
        let mut next = || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        (0..n).map(|_| [next(), next()]).collect()
    }

    fn manual(knobs: [f32; 2]) -> (OnePoleLowPass, Gain) {
        let c = Controls { knobs };
        (
            OnePoleLowPass::new(48_000.0, &filter_params(&c)),
            Gain::new(48_000.0, &gain_params(&c)),
        )
    }

    fn bits(f: &[Frame]) -> Vec<[u32; 2]> {
        f.iter().map(|x| [x[0].to_bits(), x[1].to_bits()]).collect()
    }

    #[test]
    fn matches_manual_composition_bit_for_bit() {
        for knobs in PAIRS {
            let x = input(512);
            let (mut f, mut g) = manual(knobs);
            let expected: Vec<Frame> = x.iter().map(|&s| g.tick(f.tick(s))).collect();

            let mut fx = Asperitas::new(48_000.0, &Controls { knobs });
            let ticked: Vec<Frame> = x.iter().map(|&s| fx.tick(s)).collect();
            assert_eq!(bits(&ticked), bits(&expected), "tick {knobs:?}");

            let mut fx = Asperitas::new(48_000.0, &Controls { knobs });
            let mut blk = x.clone();
            fx.process_block(&mut blk);
            assert_eq!(bits(&blk), bits(&expected), "block {knobs:?}");
        }
    }

    #[test]
    fn non_finite_input_and_knobs_stay_finite() {
        let mut fx = Asperitas::default();
        for knobs in [
            [f32::NAN, f32::NAN],
            [f32::INFINITY, f32::NEG_INFINITY],
            [-5.0, 7.0],
        ] {
            fx.set_controls(&Controls { knobs });
            let mut blk: Vec<Frame> = vec![
                [f32::NAN, f32::INFINITY],
                [f32::NEG_INFINITY, 0.5],
                [1.0, -1.0],
            ];
            blk.extend(input(256));
            fx.process_block(&mut blk);
            assert!(blk.iter().flatten().all(|v| v.is_finite()), "{knobs:?}");
        }
    }

    #[test]
    fn reset_makes_output_independent_of_history() {
        let x = input(300);
        let controls = Controls { knobs: [0.3, 0.8] };
        let mut a = Asperitas::new(48_000.0, &controls);
        let mut b = Asperitas::new(48_000.0, &controls);
        let mut warm = input(1000);
        b.process_block(&mut warm);
        a.reset();
        b.reset();
        let (mut xa, mut xb) = (x.clone(), x);
        a.process_block(&mut xa);
        b.process_block(&mut xb);
        assert_eq!(bits(&xa), bits(&xb));
    }

    #[test]
    fn sample_rate_reaches_both_stages() {
        let controls = Controls { knobs: [0.5, 0.7] };
        let x = input(256);
        let mut fx = Asperitas::new(48_000.0, &controls);
        fx.set_sample_rate(96_000.0);
        fx.set_controls(&controls);
        fx.reset();
        let mut got = x.clone();
        fx.process_block(&mut got);

        let mut f = OnePoleLowPass::new(96_000.0, &filter_params(&controls));
        let mut g = Gain::new(96_000.0, &gain_params(&controls));
        let want: Vec<Frame> = x.iter().map(|&s| g.tick(f.tick(s))).collect();
        assert_eq!(bits(&got), bits(&want));
    }

    #[test]
    fn default_controls_are_open_unity() {
        let c = Controls::default();
        assert!((FILTER_PARAMS[0].from_normalised(c.knobs[0]) - 20_000.0).abs() < 1.0);
        assert!(GAIN_PARAMS[0].from_normalised(c.knobs[1]).abs() < 1e-3);
        let mut fx = Asperitas::default();
        let mut blk: Vec<Frame> = vec![[0.5, -0.5]; 2000];
        fx.process_block(&mut blk);
        let last = blk[1999];
        assert!(
            (last[0] - 0.5).abs() < 0.01 && (last[1] + 0.5).abs() < 0.01,
            "{last:?}"
        );
    }

    #[test]
    fn from_values_without_pairs_is_default() {
        assert_eq!(Controls::from_values(&[]).unwrap(), Controls::default());
    }

    #[test]
    fn from_values_round_trips_each_knob() {
        for (i, spec) in ASPERITAS_PARAMS.iter().enumerate() {
            for frac in [0.0f32, 0.25, 0.5, 0.9, 1.0] {
                let value = spec.from_normalised(frac);
                let c = Controls::from_values(&[(spec.name, value)]).unwrap();
                let back = c.values()[i];
                assert!(
                    (back - value).abs() <= 1e-4 * value.abs().max(1.0),
                    "{} {value} -> {back}",
                    spec.name
                );
                // The other knob stays at its default.
                let other = 1 - i;
                assert_eq!(c.knobs[other], Controls::default().knobs[other]);
            }
        }
    }

    #[test]
    fn from_values_rejects_bad_input() {
        assert_eq!(
            Controls::from_values(&[("bogus", 1.0)]),
            Err(ControlsError::UnknownKey("bogus"))
        );
        for spec in &ASPERITAS_PARAMS {
            for bad in [spec.min - 1.0, spec.max + 1.0, f32::NAN, f32::INFINITY] {
                assert!(
                    matches!(
                        Controls::from_values(&[(spec.name, bad)]),
                        Err(ControlsError::OutOfRange { .. })
                    ),
                    "{} {bad}",
                    spec.name
                );
            }
        }
    }
}
