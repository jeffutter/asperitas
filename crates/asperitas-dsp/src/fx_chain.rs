//! A stimulus generator followed by the selectable [`ReplayFx`] effect.
//!
//! The rig plays one generator per build and, since TASK-019.03.05, runs the same effect after
//! whichever it is: sine, sweep, pulse or the excerpt replay. This is that pairing as a plain
//! host-testable unit, so the bypass-identity guarantee and the knob behaviour are pinned without a
//! board. Order is fixed: generator first, effect second, and the capture (the codec input) never
//! enters here.
//!
//! Bypass is the default and touches no sample, so a chain with no effect selected renders exactly
//! the bits the bare generator does. The baseline captures (TASK-034, TASK-038.05) depend on that.
//!
//! ## Callback cost
//!
//! Bypass adds one enum compare per block. With an effect selected the added work is one
//! `set_knob` compare plus 32 `Gain` or `OnePoleLowPass` ticks per block; the 2026-10-15 replay
//! measurement priced the effect at `max_block_us` 19 against 15 without it, so about 4 us of the
//! roughly 590 us of 666 us headroom. No allocation, no await, no lock held across anything.

use crate::processor::{Frame, Processor};
use crate::replay_fx::{FxKind, ReplayFx};
use crate::stimulus::Stimulus;

/// Generator `G` followed by the replay effect.
pub struct FxChain<G> {
    gen: G,
    fx: ReplayFx,
    /// The `FX` value, normalised: the pot reading a live override replaces.
    base: f32,
}

impl<G> FxChain<G> {
    /// `gen` then a bypassed effect with shipped parameter smoothing.
    pub fn new(gen: G) -> Self {
        Self {
            gen,
            fx: ReplayFx::new(),
            base: 0.0,
        }
    }

    /// As [`new`](Self::new) with smoothing removed: every parameter change steps. For the soak's
    /// negative control only; never the shipped behaviour.
    pub fn new_unsmoothed(gen: G) -> Self {
        Self {
            gen,
            fx: ReplayFx::new_unsmoothed(),
            base: 0.0,
        }
    }

    /// The generator, for callers that need something only it knows.
    pub fn inner(&self) -> &G {
        &self.gen
    }

    /// Choose effect `code` (see [`FxKind::from_code`]) at wire value `value` (`0..=65535` spans
    /// `0.0..=1.0`). `false` for an unknown code, changing nothing.
    pub fn select(&mut self, code: u8, value: u16) -> bool {
        let Some(kind) = FxKind::from_code(code) else {
            return false;
        };
        self.base = f32::from(value) / f32::from(u16::MAX);
        self.fx.select(kind, self.base);
        true
    }

    /// The selected effect.
    pub fn kind(&self) -> FxKind {
        self.fx.kind()
    }
}

impl<G: Processor> FxChain<G> {
    /// Render a block from the generator, then run the effect over it in place.
    ///
    /// `knob` maps the `FX` value to the value actually applied; the rig passes its live override
    /// merge here. It runs only when an effect is selected, so bypass stays free of it.
    pub fn process_block_with(&mut self, block: &mut [Frame], knob: impl FnOnce(f32) -> f32) {
        self.gen.process_block(block);
        if self.fx.kind() != FxKind::Bypass {
            // Upstream of smoothing; a no-op when the value is unchanged.
            self.fx.set_knob(knob(self.base));
            self.fx.process(block);
        }
    }
}

impl<G: Processor> Processor for FxChain<G> {
    type Params = G::Params;

    fn set_sample_rate(&mut self, hz: f32) {
        self.gen.set_sample_rate(hz);
        self.fx.set_sample_rate(hz);
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.gen.set_params(params);
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let mut out = [self.gen.tick(input)];
        if self.fx.kind() != FxKind::Bypass {
            self.fx.set_knob(self.base);
            self.fx.process(&mut out);
        }
        out[0]
    }

    /// Resets the generator. The effect keeps its selection and filter state: a one-shot generator
    /// is reset at capture start, and the effect was chosen before the capture armed.
    fn reset(&mut self) {
        self.gen.reset();
    }

    fn process_block(&mut self, block: &mut [Frame]) {
        self.process_block_with(block, |base| base);
    }
}

impl<G: Stimulus> Stimulus for FxChain<G> {
    /// The generator's description, plus ` fx=<name>` when an effect is selected. Bypass adds
    /// nothing, so a bypass description is byte-identical to the bare generator's.
    ///
    /// The rig emits `RIGGEN` once at boot, before any `FX` can arrive, so on the device that line
    /// always reads bypass; this is for hosts and any later re-description.
    fn describe(&self, out: &mut [u8]) -> usize {
        let n = self.gen.describe(out);
        if self.fx.kind() == FxKind::Bypass {
            return n;
        }
        let mut n = n.min(out.len());
        for part in [" fx=", self.fx.kind().name()] {
            let take = part.len().min(out.len() - n);
            out[n..n + take].copy_from_slice(&part.as_bytes()[..take]);
            n += take;
        }
        n
    }

    fn one_shot_samples(&self) -> Option<u32> {
        self.gen.one_shot_samples()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stimulus::Sine;

    fn bare() -> Sine {
        let mut s = Sine::default();
        s.set_sample_rate(48_000.0);
        s
    }

    fn chain(smooth: bool) -> FxChain<Sine> {
        let mut c = if smooth {
            FxChain::new(Sine::default())
        } else {
            FxChain::new_unsmoothed(Sine::default())
        };
        c.set_sample_rate(48_000.0);
        c
    }

    const SILENCE: [Frame; 32] = [[0.0; 2]; 32];

    fn render<P: Processor>(p: &mut P, blocks: usize) -> Vec<Frame> {
        let mut all = Vec::new();
        for _ in 0..blocks {
            let mut out = SILENCE;
            p.process_block(&mut out);
            all.extend_from_slice(&out);
        }
        all
    }

    fn peak(frames: &[Frame]) -> f32 {
        frames.iter().fold(0.0, |m, f| m.max(f[0].abs()))
    }

    #[test]
    fn bypass_is_bit_identical_to_the_bare_generator() {
        let mut c = chain(true);
        let mut b = bare();
        let (a, e) = (render(&mut c, 2000), render(&mut b, 2000));
        for (x, y) in a.iter().zip(&e) {
            assert_eq!(x[0].to_bits(), y[0].to_bits());
            assert_eq!(x[1].to_bits(), y[1].to_bits());
        }
    }

    #[test]
    fn bypass_description_is_the_generators_own() {
        let (mut a, mut b) = ([0u8; 200], [0u8; 200]);
        let (c, s) = (chain(true), bare());
        let (n, m) = (c.describe(&mut a), s.describe(&mut b));
        assert_eq!(a[..n], b[..m]);
    }

    #[test]
    fn selected_effect_is_named() {
        let mut c = chain(true);
        assert!(c.select(2, 30_000));
        let mut a = [0u8; 200];
        let n = c.describe(&mut a);
        assert!(core::str::from_utf8(&a[..n])
            .unwrap()
            .ends_with(" fx=lowpass"));
        assert!(!c.select(9, 0), "unknown code is refused");
        assert_eq!(c.kind(), FxKind::LowPass, "and changes nothing");
    }

    #[test]
    fn description_truncates_rather_than_panics() {
        let mut c = chain(true);
        c.select(1, 100);
        let mut a = [0u8; 8];
        assert_eq!(c.describe(&mut a), 8);
    }

    #[test]
    fn gain_scales_a_sine() {
        let mut c = chain(false);
        c.select(1, 0); // minimum gain
        let quiet = peak(&render(&mut c, 200)[4800..]);
        let loud = peak(&render(&mut bare(), 200)[4800..]);
        assert!(quiet < loud * 0.05, "{quiet} vs {loud}");
    }

    #[test]
    fn low_pass_attenuates_a_1khz_sine() {
        let mut c = chain(false);
        c.select(2, 0); // lowest cutoff
        let filtered = peak(&render(&mut c, 200)[4800..]);
        let raw = peak(&render(&mut bare(), 200)[4800..]);
        assert!(filtered < raw * 0.5, "{filtered} vs {raw}");
    }

    #[test]
    fn live_knob_change_moves_the_output() {
        let mut c = chain(true);
        c.select(1, u16::MAX);
        let mut out;
        let mut loud = 0.0;
        for _ in 0..200 {
            out = SILENCE;
            c.process_block_with(&mut out, |b| b);
            loud = peak(&out);
        }
        let mut quiet = 0.0;
        for _ in 0..2000 {
            out = SILENCE;
            c.process_block_with(&mut out, |_| 0.0);
            quiet = peak(&out);
        }
        assert!(quiet < loud * 0.05, "{loud} -> {quiet}");
    }

    #[test]
    fn closure_is_not_called_in_bypass() {
        let mut c = chain(true);
        let mut out = [[0.0; 2]; 32];
        c.process_block_with(&mut out, |_| panic!("override merged in bypass"));
    }

    #[test]
    fn sample_rate_reaches_the_effect() {
        let mut c = chain(false);
        c.set_sample_rate(8000.0);
        c.select(2, 30_000);
        let knob = f32::from(30_000u16) / f32::from(u16::MAX);

        let mut g = Sine::default();
        g.set_sample_rate(8000.0);
        let mut fx = ReplayFx::new_unsmoothed();
        fx.set_sample_rate(8000.0);
        fx.select(FxKind::LowPass, knob);
        let mut expected = render(&mut g, 4);
        fx.process(&mut expected);

        let got = render(&mut c, 4);
        for (x, y) in got.iter().zip(&expected) {
            assert_eq!(x[0].to_bits(), y[0].to_bits());
        }
    }

    #[test]
    fn reset_keeps_the_selection() {
        let mut c = chain(true);
        c.select(1, 100);
        c.reset();
        assert_eq!(c.kind(), FxKind::Gain);
    }
}
