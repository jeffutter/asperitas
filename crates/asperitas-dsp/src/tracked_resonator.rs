//! The resonator with modulation and a tracked root: pitch tracker, root-follow stage and
//! [`ModulatedResonator`] composed once, so every host (the offline CLI, and any later runner)
//! renders the same samples from the same controls.
//!
//! # Order of work
//!
//! [`TrackedResonator::process_block`] takes a chunk of at most [`BLOCK_FRAMES`] frames (the
//! caller splits a clip on the block grid, as `Automation::for_each_block` does). For each chunk:
//!
//! 1. the tracker is fed the mono mean of the chunk's input, non-finite samples counting as
//!    silence;
//! 2. [`RootFollow::process_block`] runs once with the tracker's estimate and confidence;
//! 3. with tracking on, the result becomes the base root of the modulated resonator;
//! 4. the chunk is processed.
//!
//! The root therefore reflects input up to and including the current chunk, and the tracker's own
//! latency (see `docs/reference/pitch-tracker.md`) and the glide of [`RootFollow`] come on top; the
//! stage does not compensate for either.
//!
//! # Tracking off
//!
//! The base root is not touched, so with every modulation depth at 0 the output is bit-identical
//! to a [`Resonator`](crate::resonator_bank::Resonator) driven by the same controls.

use crate::asperitas::BLOCK_FRAMES;
use crate::modulated_resonator::{ModulatedResonator, ModulationParams};
use crate::pitch::{PitchParams, PitchTracker};
use crate::processor::{Frame, Processor};
use crate::resonator_controls::ResonatorControls;
use crate::root_follow::{RootFollow, RootFollowParams};
use crate::smooth::sanitize;

/// The internal signals at the end of the latest block, for plotting and tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    /// The envelope follower's level, `0..1`.
    pub follower: f32,
    /// The random walk's value, `-1..1`.
    pub walk: f32,
    /// The tracker's held estimate in Hz; 0 before the first confident one.
    pub estimate_hz: f32,
    /// The tracker's clarity at its latest analysis, `0..1`.
    pub confidence: f32,
    /// The root in Hz the bank is tuned to: tracked root and modulation included.
    pub effective_root_hz: f32,
}

/// Tracker, root-follow and modulated resonator. See the module docs.
pub struct TrackedResonator {
    tracker: PitchTracker,
    root_follow: RootFollow,
    stage: ModulatedResonator,
    modulation: ModulationParams,
    follow: RootFollowParams,
    /// What was last handed to the stage, to skip a redundant `set_params`.
    applied: ModulationParams,
    mono: [f32; BLOCK_FRAMES],
}

impl TrackedResonator {
    /// A stage at `sample_rate_hz` with `controls` in force and no signal history.
    pub fn new(sample_rate_hz: f32, controls: &ResonatorControls) -> Self {
        let modulation = controls.modulation_params();
        let follow = controls.root_follow_params();
        let mut root_follow = RootFollow::new(sample_rate_hz);
        root_follow.set_params(&follow);
        Self {
            tracker: PitchTracker::new(sample_rate_hz, &PitchParams::default()),
            root_follow,
            stage: ModulatedResonator::new(sample_rate_hz, &modulation),
            follow,
            applied: modulation.clone(),
            modulation,
            mono: [0.0; BLOCK_FRAMES],
        }
    }

    /// Put `controls` in force from the next block.
    pub fn set_controls(&mut self, controls: &ResonatorControls) {
        self.modulation = controls.modulation_params();
        self.follow = controls.root_follow_params();
        self.root_follow.set_params(&self.follow);
    }

    /// Process a chunk of at most [`BLOCK_FRAMES`] frames in place.
    pub fn process_block(&mut self, block: &mut [Frame]) {
        assert!(
            block.len() <= BLOCK_FRAMES,
            "chunk of {} frames exceeds a block of {BLOCK_FRAMES}",
            block.len()
        );
        for (m, f) in self.mono.iter_mut().zip(block.iter()) {
            *m = 0.5 * (sanitize(f[0]) + sanitize(f[1]));
        }
        self.tracker.process_block(&self.mono[..block.len()]);
        let root = self
            .root_follow
            .process_block(self.tracker.estimate_hz(), self.tracker.confidence());
        let mut params = self.modulation.clone();
        if self.follow.tracking {
            params.base.root_hz = root;
        }
        if params != self.applied {
            self.stage.set_params(&params);
            self.applied = params;
        }
        self.stage.process_block(block);
    }

    /// The signals as of the latest block.
    pub fn trace(&self) -> Trace {
        Trace {
            follower: self.stage.follower_value(),
            walk: self.stage.walk_value(),
            estimate_hz: self.tracker.estimate_hz(),
            confidence: self.tracker.confidence(),
            effective_root_hz: self.stage.effective_params().root_hz,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resonator_bank::Resonator;
    use libm::sinf;

    const FS: f32 = 48_000.0;

    fn controls(values: &[(&str, f32)]) -> ResonatorControls {
        ResonatorControls::from_values(values).unwrap()
    }

    fn noise(len: usize) -> Vec<Frame> {
        let mut s = 777u32;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = (s >> 8) as f32 / (1 << 23) as f32 - 1.0;
                [0.5 * v, -0.4 * v]
            })
            .collect()
    }

    /// 0.5 s of 220 Hz then 0.5 s of 330 Hz.
    fn two_notes() -> Vec<Frame> {
        let mut phase = 0.0f32;
        (0..48_000)
            .map(|n| {
                let hz = if n < 24_000 { 220.0 } else { 330.0 };
                phase += 2.0 * core::f32::consts::PI * hz / FS;
                let v = 0.5 * sinf(phase);
                [v, v]
            })
            .collect()
    }

    fn run(c: &ResonatorControls, input: &[Frame]) -> (Vec<Frame>, Vec<Trace>) {
        let mut stage = TrackedResonator::new(FS, c);
        let mut out = input.to_vec();
        let mut traces = Vec::new();
        for chunk in out.chunks_mut(BLOCK_FRAMES) {
            stage.process_block(chunk);
            traces.push(stage.trace());
        }
        (out, traces)
    }

    fn bits(v: &[Frame]) -> Vec<[u32; 2]> {
        v.iter().map(|f| [f[0].to_bits(), f[1].to_bits()]).collect()
    }

    #[test]
    fn tracking_off_at_depth_zero_is_the_bank_bit_for_bit() {
        let c = controls(&[("root_hz", 180.0), ("damping", 0.4)]);
        let input = noise(10_000);
        let (got, _) = run(&c, &input);
        let mut bank = Resonator::new(FS, &c.params());
        let mut want = input.clone();
        for chunk in want.chunks_mut(BLOCK_FRAMES) {
            bank.set_params(&c.params());
            bank.process_block(chunk);
        }
        assert_eq!(bits(&got), bits(&want));
    }

    #[test]
    fn modulation_depth_changes_the_output() {
        let input = noise(20_000);
        let (plain, _) = run(&controls(&[]), &input);
        let (modulated, traces) = run(
            &controls(&[("follower_depth", 1.0), ("walk_depth", 1.0)]),
            &input,
        );
        assert_ne!(bits(&plain), bits(&modulated));
        assert!(traces.iter().any(|t| t.follower > 0.0));
        assert!(traces.iter().any(|t| t.walk != 0.0));
    }

    #[test]
    fn tracked_root_follows_the_note() {
        let (_, traces) = run(&controls(&[("root_source", 2.0)]), &two_notes());
        let first = traces[traces.len() / 2 - 1].effective_root_hz;
        let last = traces.last().unwrap().effective_root_hz;
        assert!((first - 220.0).abs() < 4.0, "first note: {first}");
        assert!((last - 329.63).abs() < 5.0, "second note: {last}");
    }
}
