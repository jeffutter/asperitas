//! Host override of knob readings, applied at the knob-value boundary.
//!
//! The firmware audio callback reads each knob (`KnobState` in the firmware
//! binary), then converts the normalised value to DSP parameters. Overrides
//! belong between those two steps: the injected value then flows through the
//! DSP's own parameter smoothing exactly as a turned pot would, so the host
//! exercises the real signal path. Setting a smoothed parameter directly would
//! bypass smoothing and test something the pot can never do.
//!
//! The callback calls [`KnobOverride::merge`] with the fresh knob readings and
//! uses the result. With nothing overridden, `merge` returns the readings
//! untouched (bit-identical, no arithmetic on the pass-through path).

/// Where one knob's value comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// The physical pot controls the value.
    Knob,
    /// The host dictates the value (already clamped to 0.0..=1.0).
    Host(f32),
}

/// Per-knob override state for `N` knobs.
#[derive(Clone, Debug)]
pub struct KnobOverride<const N: usize> {
    sources: [Source; N],
}

impl<const N: usize> Default for KnobOverride<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> KnobOverride<N> {
    /// All knobs under pot control.
    pub const fn new() -> Self {
        Self {
            sources: [Source::Knob; N],
        }
    }

    /// Override knob `idx` with `value`, clamped to 0.0..=1.0 as
    /// `knob::normalize` does. Returns `false` and changes nothing when `idx`
    /// is out of range or `value` is NaN.
    pub fn set(&mut self, idx: usize, value: f32) -> bool {
        if value.is_nan() {
            return false;
        }
        match self.sources.get_mut(idx) {
            Some(s) => {
                *s = Source::Host(value.clamp(0.0, 1.0));
                true
            }
            None => false,
        }
    }

    /// Return knob `idx` to pot control. Out-of-range `idx` is ignored.
    pub fn release(&mut self, idx: usize) {
        if let Some(s) = self.sources.get_mut(idx) {
            *s = Source::Knob;
        }
    }

    /// Return every knob to pot control.
    #[cfg(test)]
    pub fn release_all(&mut self) {
        self.sources = [Source::Knob; N];
    }

    /// Whether knob `idx` is currently host-controlled.
    #[cfg(test)]
    pub fn is_active(&self, idx: usize) -> bool {
        matches!(self.sources.get(idx), Some(Source::Host(_)))
    }

    /// Resolve the value for each knob: the host value where overridden, the
    /// pot reading otherwise.
    pub fn merge(&self, knobs: [f32; N]) -> [f32; N] {
        let mut out = knobs;
        for (o, s) in out.iter_mut().zip(self.sources.iter()) {
            if let Source::Host(v) = s {
                *o = *v;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::{vec, vec::Vec};

    use super::*;
    use crate::knob::normalize;
    use asperitas_dsp::{Gain, Processor};

    #[test]
    fn merge_without_override_is_bit_identical() {
        let mut ov = KnobOverride::<2>::new();
        let mut inputs = vec![0.0f32, 1.0, 0.5, f32::MIN_POSITIVE / 4.0, -0.0, 1.0e-30];
        for raw in [0u16, 1, 100, 2048, 4095, 65535] {
            inputs.push(normalize(raw));
        }
        let check = |ov: &KnobOverride<2>| {
            for w in inputs.windows(2) {
                let got = ov.merge([w[0], w[1]]);
                assert_eq!(got[0].to_bits(), w[0].to_bits());
                assert_eq!(got[1].to_bits(), w[1].to_bits());
            }
        };
        check(&ov);
        ov.set(0, 0.7);
        ov.release(0);
        check(&ov);
        ov.set(1, 0.2);
        ov.release_all();
        check(&ov);
    }

    #[test]
    fn set_clamps_and_is_per_knob() {
        let mut ov = KnobOverride::<3>::new();
        assert!(ov.set(1, 1.5));
        assert_eq!(ov.merge([0.3, 0.3, 0.3]), [0.3, 1.0, 0.3]);
        assert!(ov.set(1, -0.2));
        assert_eq!(ov.merge([0.3, 0.3, 0.3]), [0.3, 0.0, 0.3]);
        assert!(ov.is_active(1));
        assert!(!ov.is_active(0));
        ov.release(1);
        assert!(!ov.is_active(1));
    }

    #[test]
    fn nan_and_out_of_range_are_ignored() {
        let mut ov = KnobOverride::<2>::new();
        assert!(!ov.set(0, f32::NAN));
        assert!(!ov.set(2, 0.5));
        ov.release(9);
        assert!(!ov.is_active(0));
        assert_eq!(ov.merge([0.1, 0.2]), [0.1, 0.2]);
    }

    /// Settled gain at knob 0.0, then drive to knob 1.0 two ways.
    fn settled_gain() -> Gain {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&Gain::params_from_normalised(0.0));
        g.reset();
        g
    }

    fn run(g: &mut Gain, n: usize) -> Vec<f32> {
        (0..n).map(|_| g.tick([0.1, 0.1])[0]).collect()
    }

    #[test]
    fn knob_boundary_injection_is_smoothed_direct_set_is_not() {
        // Path A: override at the knob boundary, then the normal params path.
        let mut ov = KnobOverride::<1>::new();
        ov.set(0, 1.0);
        let mut a = settled_gain();
        let [k] = ov.merge([0.0]);
        a.set_params(&Gain::params_from_normalised(k));
        let a_out = run(&mut a, 6000);

        // Path B: same target, but the smoother is snapped to it.
        let mut b = settled_gain();
        b.set_params(&Gain::params_from_normalised(1.0));
        b.reset();
        let b_out = run(&mut b, 6000);

        // B is at the final gain immediately; A starts far below it.
        assert!(a_out[0] < b_out[0] * 0.1, "{} vs {}", a_out[0], b_out[0]);
        // A ramps monotonically and converges onto B.
        assert!(a_out.windows(2).all(|w| w[1] >= w[0]));
        let last = a_out[a_out.len() - 1];
        assert!((last - b_out[5999]).abs() < b_out[5999] * 0.01);
    }
}
