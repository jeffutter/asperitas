//! A stimulus generator followed by the selectable [`ReplayFx`] stage.
//!
//! The rig plays one generator per build and runs the same stage after whichever it is: sine,
//! sweep, pulse or the excerpt replay. The stage is bypass or the [`Asperitas`](crate::Asperitas)
//! patch `main` runs. This is that pairing as a plain host-testable unit, so the bypass-identity
//! guarantee and the knob behaviour are pinned without a board. Order is fixed: generator first,
//! stage second, and the capture (the codec input) never enters here.
//!
//! Bypass is the default and touches no sample, so a chain with nothing selected renders exactly
//! the bits the bare generator does. The baseline captures depend on that.
//!
//! ## Callback cost
//!
//! Bypass adds one enum compare per block. With the patch selected the added work is one
//! `set_knobs` compare plus the filter and gain over 32 frames per block. No allocation, no
//! await, no lock held across anything.

use crate::asperitas::{Controls, KNOB_COUNT};
use crate::processor::{Frame, Processor};
use crate::replay_fx::{FxKind, ReplayFx};
use crate::stimulus::Stimulus;

/// Generator `G` followed by the replay stage.
pub struct FxChain<G> {
    gen: G,
    fx: ReplayFx,
    /// The knob positions a live override replaces: the patch's defaults.
    base: [f32; KNOB_COUNT],
}

impl<G> FxChain<G> {
    /// `gen` then a bypassed stage with shipped parameter smoothing.
    pub fn new(gen: G) -> Self {
        Self {
            gen,
            fx: ReplayFx::new(),
            base: Controls::default().knobs,
        }
    }

    /// As [`new`](Self::new) with smoothing removed: every parameter change steps. For the soak's
    /// negative control only; never the shipped behaviour.
    pub fn new_unsmoothed(gen: G) -> Self {
        Self {
            gen,
            fx: ReplayFx::new_unsmoothed(),
            base: Controls::default().knobs,
        }
    }

    /// The generator, for callers that need something only it knows.
    pub fn inner(&self) -> &G {
        &self.gen
    }

    /// Choose stage `code` (see [`FxKind::from_code`]). The patch starts at its default knob
    /// positions; `_value` is accepted so the wire shape stays stable and is ignored. `false` for
    /// an unknown code, changing nothing.
    pub fn select(&mut self, code: u8, _value: u16) -> bool {
        let Some(kind) = FxKind::from_code(code) else {
            return false;
        };
        self.base = Controls::default().knobs;
        self.fx.select(kind, self.base);
        true
    }

    /// The selected stage.
    pub fn kind(&self) -> FxKind {
        self.fx.kind()
    }
}

impl<G: Processor> FxChain<G> {
    /// Render a block from the generator, then run the stage over it in place.
    ///
    /// `knobs` maps the patch's default knob positions to the ones actually applied; the rig
    /// passes its live override merge here. It runs only when the patch is selected, so bypass
    /// stays free of it.
    pub fn process_block_with(
        &mut self,
        block: &mut [Frame],
        knobs: impl FnOnce([f32; KNOB_COUNT]) -> [f32; KNOB_COUNT],
    ) {
        self.gen.process_block(block);
        if self.fx.kind() != FxKind::Bypass {
            // Upstream of smoothing; a no-op when the values are unchanged.
            self.fx.set_knobs(knobs(self.base));
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
            self.fx.set_knobs(self.base);
            self.fx.process(&mut out);
        }
        out[0]
    }

    /// Resets the generator only. This deliberately does not meet the [`Processor::reset`] law
    /// (identical input gives identical output regardless of history): the effect keeps its
    /// selection, smoothed parameters and filter state, so output right after a reset still
    /// depends on what the effect heard before. A one-shot generator is reset at capture start,
    /// and the effect was chosen before the capture armed.
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
        assert!(c.select(1, 30_000));
        let mut a = [0u8; 200];
        let n = c.describe(&mut a);
        assert!(core::str::from_utf8(&a[..n])
            .unwrap()
            .ends_with(" fx=patch"));
        assert!(!c.select(9, 0), "unknown code is refused");
        assert_eq!(c.kind(), FxKind::Patch, "and changes nothing");
    }

    #[test]
    fn description_truncates_rather_than_panics() {
        let mut c = chain(true);
        c.select(1, 100);
        let mut a = [0u8; 8];
        assert_eq!(c.describe(&mut a), 8);
    }

    #[test]
    fn default_patch_passes_a_1khz_sine_at_unity() {
        let mut c = chain(false);
        c.select(1, 0);
        let patched = peak(&render(&mut c, 200)[4800..]);
        let raw = peak(&render(&mut bare(), 200)[4800..]);
        assert!((patched / raw - 1.0).abs() < 0.01, "{patched} vs {raw}");
    }

    #[test]
    fn gain_knob_scales_a_sine() {
        let mut c = chain(false);
        c.select(1, 0);
        let mut quiet = 0.0;
        for _ in 0..200 {
            let mut out = SILENCE;
            c.process_block_with(&mut out, |b| [b[0], 0.0]); // minimum gain
            quiet = peak(&out);
        }
        let loud = peak(&render(&mut bare(), 200)[4800..]);
        assert!(quiet < loud * 0.05, "{quiet} vs {loud}");
    }

    #[test]
    fn cutoff_knob_attenuates_a_1khz_sine() {
        let mut c = chain(false);
        c.select(1, 0);
        let mut filtered = 0.0;
        for _ in 0..200 {
            let mut out = SILENCE;
            c.process_block_with(&mut out, |b| [0.0, b[1]]); // lowest cutoff
            filtered = peak(&out);
        }
        let raw = peak(&render(&mut bare(), 200)[4800..]);
        assert!(filtered < raw * 0.5, "{filtered} vs {raw}");
    }

    #[test]
    fn live_knob_change_moves_the_output() {
        let mut c = chain(true);
        c.select(1, 0);
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
            c.process_block_with(&mut out, |b| [b[0], 0.0]);
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
        c.select(1, 0);

        let mut g = Sine::default();
        g.set_sample_rate(8000.0);
        let mut fx = ReplayFx::new_unsmoothed();
        fx.set_sample_rate(8000.0);
        fx.select(FxKind::Patch, Controls::default().knobs);
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
        assert_eq!(c.kind(), FxKind::Patch);
    }

    #[test]
    fn reset_restarts_the_generator_but_not_the_effect() {
        // Drive the patch to a low cutoff so the filter holds a lot of state, then reset.
        let mut c = chain(false);
        c.select(1, 0);
        for _ in 0..200 {
            let mut out = SILENCE;
            c.process_block_with(&mut out, |b| [b[0], 0.0]);
        }
        let mut twin = chain(false);
        twin.select(1, 0);
        c.reset();

        let after_reset = {
            let mut out = SILENCE;
            c.process_block_with(&mut out, |b| [b[0], 0.0]);
            out
        };
        let fresh = {
            let mut out = SILENCE;
            twin.process_block_with(&mut out, |b| [b[0], 0.0]);
            out
        };
        // The generator restarted, so the unfiltered first sample is the fresh one's; the
        // effect did not, so the settled low-cutoff filter state makes the blocks differ. A
        // chain that honoured the Processor reset law would make them identical.
        assert_eq!(c.kind(), FxKind::Patch);
        assert_ne!(
            after_reset.map(|f| f[0].to_bits()),
            fresh.map(|f| f[0].to_bits())
        );
    }
}
