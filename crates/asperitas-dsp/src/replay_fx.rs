//! The rig's selectable effect stage: bypass, or the [`Asperitas`] patch `main` runs.
//!
//! Bypass touches no sample, so the output stays bit-exact with the bare generator or replay.
//! Patch is the same filter-then-gain chain, driven by the same two normalised knobs
//! ([`Controls`]), so the rig measures what `main` ships and no knob mapping lives outside
//! `asperitas-dsp`.

use crate::asperitas::{Asperitas, Controls, KNOB_COUNT};
use crate::processor::{Frame, Processor};

/// Which stage the replay runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    Bypass,
    Patch,
}

impl FxKind {
    /// Map the wire code (`0` bypass, `1` patch); `None` for anything else.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(FxKind::Bypass),
            1 => Some(FxKind::Patch),
            _ => None,
        }
    }

    /// The name `describe()` reports for a non-bypass effect.
    pub fn name(self) -> &'static str {
        match self {
            FxKind::Bypass => "bypass",
            FxKind::Patch => "patch",
        }
    }
}

/// The effect stage. Default is bypass.
pub struct ReplayFx {
    kind: FxKind,
    patch: Asperitas,
    smoothing: bool,
    /// The knob positions the patch was last parameterised from.
    knobs: [f32; KNOB_COUNT],
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
            patch: Asperitas::default(),
            smoothing: true,
            knobs: Controls::default().knobs,
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

    /// Set the sample rate and re-derive the current parameters at it, keeping filter history.
    /// A non-positive rate is ignored.
    pub fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 {
            self.patch.set_sample_rate(hz);
            if !self.smoothing {
                self.patch.snap_params();
            }
        }
    }

    /// Re-parameterise the patch from `knobs` without resetting it: filter history and any
    /// in-flight smoothing ramp carry on, so a sweep is continuous. Bypass: a no-op that touches
    /// nothing. When built unsmoothed the new values are snapped in. Knobs equal to the ones
    /// already applied do nothing, so it is cheap to call every block.
    pub fn set_knobs(&mut self, knobs: [f32; KNOB_COUNT]) {
        if knobs == self.knobs {
            return;
        }
        self.knobs = knobs;
        if self.kind == FxKind::Patch {
            self.patch.set_controls(&Controls { knobs });
            if !self.smoothing {
                self.patch.snap_params();
            }
        }
    }

    /// Choose the stage and, for patch, set its knobs. Patch starts from a clean state with no
    /// smoothing ramp, so the same selection always produces the same output.
    pub fn select(&mut self, kind: FxKind, knobs: [f32; KNOB_COUNT]) {
        self.kind = kind;
        self.knobs = knobs;
        if kind == FxKind::Patch {
            self.patch.set_controls(&Controls { knobs });
            self.patch.reset();
        }
    }

    /// The current selection.
    pub fn kind(&self) -> FxKind {
        self.kind
    }

    /// Process `frames` in place. No allocation; bypass returns before reading a sample.
    pub fn process(&mut self, frames: &mut [Frame]) {
        if self.kind == FxKind::Patch {
            self.patch.process_block(frames);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: [f32; KNOB_COUNT] = [0.0; KNOB_COUNT];

    fn defaults() -> [f32; KNOB_COUNT] {
        Controls::default().knobs
    }

    fn ramp() -> [Frame; 32] {
        let mut b = [[0.0; 2]; 32];
        for (i, f) in b.iter_mut().enumerate() {
            let v = (i as f32 - 16.0) / 32.0;
            *f = [v, v];
        }
        b
    }

    fn bits(f: &[Frame]) -> Vec<[u32; 2]> {
        f.iter().map(|x| [x[0].to_bits(), x[1].to_bits()]).collect()
    }

    fn run_block(fx: &mut ReplayFx) -> [Frame; 32] {
        let mut b = [[0.1; 2]; 32];
        fx.process(&mut b);
        b
    }

    #[test]
    fn default_is_bit_exact_bypass() {
        let mut fx = ReplayFx::new();
        let mut b = ramp();
        fx.process(&mut b);
        assert_eq!(bits(&b), bits(&ramp()));
    }

    #[test]
    fn patch_equals_the_asperitas_it_wraps() {
        for knobs in [defaults(), [0.2, 0.5], [0.9, 0.1]] {
            let mut fx = ReplayFx::new();
            fx.select(FxKind::Patch, knobs);
            let mut want = Asperitas::new(48_000.0, &Controls { knobs });
            let (mut a, mut e) = (ramp(), ramp());
            fx.process(&mut a);
            want.process_block(&mut e);
            assert_eq!(bits(&a), bits(&e), "{knobs:?}");
        }
    }

    #[test]
    fn low_gain_attenuates() {
        // Direction only: the dB mapping is the parameter table's own, with its own tests.
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Patch, [1.0, 0.5]);
        let mut b = ramp();
        fx.process(&mut b);
        assert!(b[31][0] > 0.0 && b[31][0] < ramp()[31][0], "{}", b[31][0]);
    }

    #[test]
    fn low_cutoff_alters_output() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Patch, [0.2, defaults()[1]]);
        let mut b = ramp();
        fx.process(&mut b);
        assert_ne!(b[31][0].to_bits(), ramp()[31][0].to_bits());
    }

    #[test]
    fn returning_to_bypass_restores_exact_output() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Patch, [0.1, 0.1]);
        fx.select(FxKind::Bypass, DEFAULTS);
        let mut b = ramp();
        fx.process(&mut b);
        assert_eq!(bits(&b), bits(&ramp()));
    }

    #[test]
    fn set_knobs_moves_output_between_blocks() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Patch, [1.0, 1.0]);
        let loud = run_block(&mut fx)[31][0];
        fx.set_knobs([1.0, 0.2]);
        for _ in 0..200 {
            run_block(&mut fx);
        }
        let quiet = run_block(&mut fx)[31][0];
        assert!(quiet < loud * 0.1, "{loud} -> {quiet}");
    }

    #[test]
    fn set_knobs_on_bypass_is_bit_exact_noop() {
        let mut fx = ReplayFx::new();
        fx.set_knobs([0.3, 0.3]);
        let mut b = ramp();
        fx.process(&mut b);
        assert_eq!(bits(&b), bits(&ramp()));
    }

    #[test]
    fn unsmoothed_steps_within_one_sample_smoothed_ramps() {
        let mut smooth = ReplayFx::new();
        let mut step = ReplayFx::new_unsmoothed();
        for fx in [&mut smooth, &mut step] {
            fx.select(FxKind::Patch, [1.0, 1.0]);
            // Let the open filter settle on the constant input so only the gain ramp is left.
            run_block(fx);
            run_block(fx);
            fx.set_knobs([1.0, 0.5]);
        }
        let target = 0.1 * 10f32.powf((-60.0 + 0.5 * 72.0) / 20.0);
        let s = run_block(&mut smooth);
        let u = run_block(&mut step);
        assert!((u[0][0] - target).abs() < 1e-3, "{} vs {target}", u[0][0]);
        assert!(
            s[0][0] > target * 2.0,
            "smoothed starts far from target: {}",
            s[0][0]
        );
        assert!(s[31][0] < s[0][0], "smoothed is still ramping");
    }

    #[test]
    fn set_knobs_with_unchanged_values_leaves_ramp_alone() {
        let mut fx = ReplayFx::new();
        fx.select(FxKind::Patch, [1.0, 1.0]);
        fx.set_knobs([1.0, 0.5]);
        let a = run_block(&mut fx)[31][0];
        fx.set_knobs([1.0, 0.5]);
        let b = run_block(&mut fx)[31][0];
        assert!(b < a, "ramp continues: {a} -> {b}");
    }

    #[test]
    fn set_knobs_keeps_filter_history() {
        let mut fx = ReplayFx::new_unsmoothed();
        fx.select(FxKind::Patch, [0.5, defaults()[1]]);
        run_block(&mut fx);
        fx.set_knobs([0.6, defaults()[1]]);
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
        assert_eq!(FxKind::from_code(1), Some(FxKind::Patch));
        assert_eq!(FxKind::from_code(2), None);
    }
}
