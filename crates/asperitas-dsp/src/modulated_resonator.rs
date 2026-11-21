//! The resonator with an envelope follower and a random walk modulating its parameters.
//!
//! The routing is the owner's decision recorded in
//! `backlog/decisions/decision-002 - M6-modulation-routing-pod-mapping-and-tracked-root.md`: the
//! follower modulates damping (unipolar, it only lengthens the ring from the base value) and the
//! random walk modulates spread (bipolar around the base value), both with a squared depth law.
//! The routing is the constant table [`MODULATION_ROUTES`], so a target change is a one-line edit.
//! The page `docs/reference/resonator.md` documents the stage.
//!
//! # Signal flow
//!
//! [`ModulatedResonator`] owns a [`Resonator`], an [`EnvelopeFollower`] and a [`RandomWalk`]. Every
//! sample the follower is fed the mean of the two input channels (non-finite samples count as
//! silence) and the walk advances, so both sequences are independent of how the samples are split
//! into blocks. Every [`BLOCK_FRAMES`] frames the effective parameters are computed from the base
//! parameters and the two source values and applied with [`Resonator::set_params`]; the bank's own
//! smoothing keeps the change click-free. The audio path is the bank's, unchanged.
//!
//! # Depth law
//!
//! Each source has a depth in `[0, 1]` (NaN counts as 0). The effective depth is `depth^2`. For a
//! target the offset is `sign * depth^2 * source`, where the follower's source is its level
//! clamped to `[0, 1]` and the walk's is its value in `[-1, 1]`, and `sign` follows the route's
//! [`Polarity`]. The offset is added to the base value in the unit of the target (the parameter
//! range for structure, damping and spread, [`ROOT_CENTS_SPAN`] cents for the root) and the sum is
//! clamped to the parameter's [`ParamSpec`] range in [`RESONATOR_PARAMS`].
//!
//! # Depth 0
//!
//! A target whose offset is exactly zero keeps the base value's bits, so with every depth 0 the
//! effective parameters equal the base parameters and the output equals [`Resonator`]'s bit for
//! bit. [`Resonator::set_params`] is called again only when the effective values differ bitwise
//! from the last applied ones.
//!
//! # Bound
//!
//! Modulation only changes parameter values, and the bank's bound holds at any parameters, so
//! `|out| <= BANK_GAIN * VOICE_COUNT * min(2.0, 6 * input_peak)` (at most 32) for any depth,
//! parameters and input.

use crate::asperitas::BLOCK_FRAMES;
use crate::envelope::{EnvelopeFollower, EnvelopeParams};
use crate::param::ParamSpec;
use crate::processor::{Frame, Processor};
use crate::random_walk::{RandomWalk, RandomWalkParams};
use crate::resonator_bank::{Resonator, ResonatorParams, RESONATOR_PARAMS};
use crate::smooth::sanitize;
use libm::exp2f;

/// Seed of the random walk, fixed so a render is reproducible.
pub const MODULATION_WALK_SEED: u32 = 0x5eed_0002;

/// Cents a root-targeted source moves the root at full effective depth.
pub const ROOT_CENTS_SPAN: f32 = 100.0;

/// A modulation source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The envelope follower's level, `0..1`.
    Follower,
    /// The random walk, `-1..1`.
    Walk,
}

/// The parameter a source moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Root pitch, in cents.
    Root,
    Structure,
    Damping,
    Spread,
}

impl Target {
    fn index(self) -> usize {
        match self {
            Target::Root => 0,
            Target::Structure => 1,
            Target::Damping => 2,
            Target::Spread => 3,
        }
    }
}

/// How a source's value is applied to the base value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    /// Adds the source value: a positive source raises the parameter.
    Up,
    /// Subtracts the source value: a positive source lowers the parameter.
    Down,
}

/// One routing: a source moves a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub source: Source,
    pub target: Target,
    pub polarity: Polarity,
}

/// The routing table: the follower lowers damping (a longer ring), the walk moves spread both ways.
pub const MODULATION_ROUTES: [Route; 2] = [
    Route {
        source: Source::Follower,
        target: Target::Damping,
        polarity: Polarity::Down,
    },
    Route {
        source: Source::Walk,
        target: Target::Spread,
        polarity: Polarity::Up,
    },
];

/// Parameters for [`ModulatedResonator`].
#[derive(Clone, Debug, PartialEq)]
pub struct ModulationParams {
    /// The unmodulated resonator parameters.
    pub base: ResonatorParams,
    /// Depth of the follower route, `0..1` (squared before use). NaN counts as 0.
    pub follower_depth: f32,
    /// Depth of the random walk route, `0..1` (squared before use). NaN counts as 0.
    pub walk_depth: f32,
    /// Attack and release of the follower.
    pub follower: EnvelopeParams,
    /// Rate of the random walk.
    pub walk: RandomWalkParams,
}

impl Default for ModulationParams {
    fn default() -> Self {
        Self {
            base: ResonatorParams::default(),
            follower_depth: 0.0,
            walk_depth: 0.0,
            follower: EnvelopeParams::default(),
            walk: RandomWalkParams::default(),
        }
    }
}

fn depth_eff(depth: f32) -> f32 {
    if depth.is_nan() {
        0.0
    } else {
        let d = depth.clamp(0.0, 1.0);
        d * d
    }
}

/// `value` with a NaN replaced by the spec's default, then clamped to the spec's range.
fn clamp_to_spec(value: f32, spec: &ParamSpec) -> f32 {
    if value.is_nan() {
        spec.default
    } else {
        value.clamp(spec.min, spec.max)
    }
}

/// The resonator with modulated parameters. See the module docs.
pub struct ModulatedResonator {
    bank: Resonator,
    follower: EnvelopeFollower,
    walk: RandomWalk,
    params: ModulationParams,
    /// Root, structure, damping and spread last handed to the bank.
    applied: [f32; 4],
    frames: usize,
}

impl Default for ModulatedResonator {
    fn default() -> Self {
        Self::new(48_000.0, &ModulationParams::default())
    }
}

impl ModulatedResonator {
    /// A stage at `sample_rate_hz` with `params` in force and no signal history.
    pub fn new(sample_rate_hz: f32, params: &ModulationParams) -> Self {
        let mut s = Self {
            bank: Resonator::new(sample_rate_hz, &params.base),
            follower: EnvelopeFollower::new(sample_rate_hz, &params.follower),
            walk: RandomWalk::new(sample_rate_hz, MODULATION_WALK_SEED, &params.walk),
            params: params.clone(),
            applied: Self::base_values(&params.base),
            frames: 0,
        };
        s.set_params(params);
        s.reset();
        s
    }

    fn base_values(p: &ResonatorParams) -> [f32; 4] {
        [p.root_hz, p.structure, p.damping, p.spread]
    }

    /// The root, structure, damping and spread the bank currently targets, as a parameter set
    /// (the excitation is the base's).
    pub fn effective_params(&self) -> ResonatorParams {
        ResonatorParams {
            root_hz: self.applied[0],
            structure: self.applied[1],
            damping: self.applied[2],
            spread: self.applied[3],
            excitation: self.params.base.excitation.clone(),
        }
    }

    /// The envelope follower's current level, for inspection.
    pub fn follower_value(&self) -> f32 {
        self.follower.value()
    }

    /// The random walk's current value, for inspection.
    pub fn walk_value(&self) -> f32 {
        self.walk.value()
    }

    /// Base values with every route's offset added; a target with no offset keeps the base bits.
    fn compute_effective(&self) -> [f32; 4] {
        let mut v = Self::base_values(&self.params.base);
        let mut offset = [0.0f32; 4];
        for route in &MODULATION_ROUTES {
            let (depth, signal) = match route.source {
                Source::Follower => (
                    self.params.follower_depth,
                    sanitize(self.follower.value()).clamp(0.0, 1.0),
                ),
                Source::Walk => (self.params.walk_depth, self.walk.value().clamp(-1.0, 1.0)),
            };
            let sign = match route.polarity {
                Polarity::Up => 1.0,
                Polarity::Down => -1.0,
            };
            offset[route.target.index()] += sign * depth_eff(depth) * signal;
        }
        for (i, o) in offset.iter().enumerate() {
            if *o == 0.0 {
                continue;
            }
            let spec = &RESONATOR_PARAMS[i];
            let base = clamp_to_spec(v[i], spec);
            v[i] = clamp_to_spec(
                if i == Target::Root.index() {
                    base * exp2f(o * ROOT_CENTS_SPAN / 1200.0)
                } else {
                    base + o
                },
                spec,
            );
        }
        v
    }

    /// Hand the effective parameters to the bank when they differ bitwise from the last set.
    fn apply_effective(&mut self) {
        let v = self.compute_effective();
        if v.iter()
            .zip(&self.applied)
            .all(|(a, b)| a.to_bits() == b.to_bits())
        {
            return;
        }
        self.applied = v;
        self.bank.set_params(&self.effective_params());
    }
}

impl Processor for ModulatedResonator {
    type Params = ModulationParams;

    fn set_sample_rate(&mut self, hz: f32) {
        self.bank.set_sample_rate(hz);
        self.follower.set_sample_rate(hz);
        self.walk.set_sample_rate(hz);
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.params = params.clone();
        self.follower.set_params(&params.follower);
        self.walk.set_params(&params.walk);
        // The bank is always told, so a changed excitation or base reaches it even when the
        // effective four values are unchanged.
        self.applied = self.compute_effective();
        self.bank.set_params(&self.effective_params());
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let mono = 0.5 * (sanitize(input[0]) + sanitize(input[1]));
        self.follower.tick(mono);
        self.walk.tick();
        let out = self.bank.tick(input);
        self.frames += 1;
        if self.frames == BLOCK_FRAMES {
            self.frames = 0;
            self.apply_effective();
        }
        out
    }

    fn reset(&mut self) {
        self.bank.reset();
        self.follower.reset();
        self.walk.reset();
        self.frames = 0;
        self.applied = self.compute_effective();
        self.bank.set_params(&self.effective_params());
        self.bank.snap_params();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excitation::ExcitationParams;
    use crate::resonator_bank::{BANK_GAIN, VOICE_COUNT};
    use libm::{fabsf, sinf};

    const FS: f32 = 48_000.0;

    fn noise(len: usize) -> Vec<Frame> {
        let mut s = 12_345u32;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = (s >> 8) as f32 / (1 << 23) as f32 - 1.0;
                [0.5 * v, -0.4 * v]
            })
            .collect()
    }

    fn tone(len: usize) -> Vec<Frame> {
        (0..len)
            .map(|n| {
                let v = 0.5 * sinf(2.0 * core::f32::consts::PI * 220.0 * n as f32 / FS);
                [v, v]
            })
            .collect()
    }

    fn base() -> ResonatorParams {
        ResonatorParams {
            root_hz: 150.0,
            structure: 0.3,
            damping: 0.5,
            spread: 0.5,
            ..ResonatorParams::default()
        }
    }

    fn mp(follower_depth: f32, walk_depth: f32) -> ModulationParams {
        ModulationParams {
            base: base(),
            follower_depth,
            walk_depth,
            follower: EnvelopeParams {
                attack_ms: 5.0,
                release_ms: 200.0,
            },
            walk: RandomWalkParams { rate_hz: 2.0 },
        }
    }

    fn bits(v: &[Frame]) -> Vec<[u32; 2]> {
        v.iter().map(|f| [f[0].to_bits(), f[1].to_bits()]).collect()
    }

    fn render<P: Processor>(m: &mut P, input: &[Frame]) -> Vec<Frame> {
        input.iter().map(|f| m.tick(*f)).collect()
    }

    #[test]
    fn depth_zero_is_bit_identical_to_the_bank() {
        for input in [noise(20_000), tone(20_000)] {
            let want = render(&mut Resonator::new(FS, &base()), &input);
            let got = render(&mut ModulatedResonator::new(FS, &mp(0.0, 0.0)), &input);
            assert_eq!(bits(&got), bits(&want));
        }
    }

    #[test]
    fn depth_zero_with_nan_depths_is_also_identical() {
        let input = noise(5_000);
        let want = render(&mut Resonator::new(FS, &base()), &input);
        let got = render(
            &mut ModulatedResonator::new(FS, &mp(f32::NAN, f32::NAN)),
            &input,
        );
        assert_eq!(bits(&got), bits(&want));
    }

    /// Negative control: the same comparison fails when any one depth is non-zero.
    #[test]
    fn a_nonzero_depth_breaks_the_identity() {
        let input = noise(20_000);
        let want = bits(&render(&mut Resonator::new(FS, &base()), &input));
        for (f, w) in [(0.8, 0.0), (0.0, 0.8)] {
            let got = render(&mut ModulatedResonator::new(FS, &mp(f, w)), &input);
            assert_ne!(bits(&got), want, "depths {f} {w}");
        }
    }

    fn dc(len: usize, level: f32) -> Vec<Frame> {
        vec![[level, level]; len]
    }

    /// Settled follower level `x` at depth `d` gives damping `base - d^2 * x`.
    #[test]
    fn follower_lowers_damping_by_depth_squared_times_level() {
        let d = 0.6;
        let loud = {
            let mut m = ModulatedResonator::new(FS, &mp(d, 0.0));
            render(&mut m, &dc(4_000, 0.8));
            m.effective_params().damping
        };
        let quiet = {
            let mut m = ModulatedResonator::new(FS, &mp(d, 0.0));
            render(&mut m, &dc(4_000, 0.1));
            m.effective_params().damping
        };
        assert!(fabsf(loud - (0.5 - d * d * 0.8)) < 2e-3, "loud {loud}");
        assert!(fabsf(quiet - (0.5 - d * d * 0.1)) < 2e-3, "quiet {quiet}");
        assert!(loud < quiet && quiet < 0.5);
        // The other targets stay at the base value, bit for bit.
        let m = ModulatedResonator::new(FS, &mp(d, 0.0));
        let e = m.effective_params();
        assert_eq!(e.spread.to_bits(), 0.5f32.to_bits());
        assert_eq!(e.root_hz.to_bits(), 150.0f32.to_bits());
        assert_eq!(e.structure.to_bits(), 0.3f32.to_bits());
    }

    #[test]
    fn follower_never_raises_damping_above_base() {
        let mut m = ModulatedResonator::new(FS, &mp(1.0, 0.0));
        for (i, f) in noise(48_000).iter().enumerate() {
            m.tick(*f);
            if i % BLOCK_FRAMES == 0 {
                assert!(m.effective_params().damping <= 0.5);
                assert!(m.effective_params().damping >= 0.0);
            }
        }
    }

    /// The effective spread follows `base + d^2 * walk` sampled at every block end, in both
    /// directions, within `d^2` of the base and inside the parameter range.
    #[test]
    fn walk_moves_spread_around_the_base_both_ways() {
        let d = 0.7;
        let amp = d * d;
        let mut m = ModulatedResonator::new(FS, &mp(0.0, d));
        let mut reference =
            RandomWalk::new(FS, MODULATION_WALK_SEED, &RandomWalkParams { rate_hz: 2.0 });
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let silence = [0.0, 0.0];
        for block in 0..(10 * 48_000 / BLOCK_FRAMES) {
            for _ in 0..BLOCK_FRAMES {
                m.tick(silence);
                reference.tick();
            }
            let s = m.effective_params().spread;
            let want = (0.5 + amp * reference.value()).clamp(0.0, 1.0);
            assert!(fabsf(s - want) < 1e-6, "block {block}: {s} vs {want}");
            assert!(fabsf(s - 0.5) <= amp + 1e-6);
            assert!((0.0..=1.0).contains(&s));
            lo = lo.min(s);
            hi = hi.max(s);
        }
        assert!(lo < 0.5 - 0.15 && hi > 0.5 + 0.15, "range {lo}..{hi}");
    }

    #[test]
    fn walk_depth_clamps_the_spread_to_the_range() {
        let mut m = ModulatedResonator::new(
            FS,
            &ModulationParams {
                base: ResonatorParams {
                    spread: 0.95,
                    ..base()
                },
                ..mp(0.0, 1.0)
            },
        );
        for _ in 0..(10 * 48_000) {
            m.tick([0.0, 0.0]);
            let s = m.effective_params().spread;
            assert!((0.0..=1.0).contains(&s));
        }
    }

    /// Both routes together, with the root and structure still at the base.
    #[test]
    fn untargeted_parameters_keep_their_base() {
        let mut m = ModulatedResonator::new(FS, &mp(1.0, 1.0));
        for f in noise(48_000) {
            m.tick(f);
        }
        let e = m.effective_params();
        assert_eq!(e.root_hz.to_bits(), 150.0f32.to_bits());
        assert_eq!(e.structure.to_bits(), 0.3f32.to_bits());
        assert_eq!(e.excitation, base().excitation);
    }

    #[test]
    fn hostile_values_stay_finite_and_under_the_bound() {
        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let cases = [
            mp(nan, nan),
            mp(inf, -inf),
            mp(1e30, -1e30),
            mp(1.0, 1.0),
            ModulationParams {
                base: ResonatorParams {
                    root_hz: nan,
                    structure: inf,
                    damping: -inf,
                    spread: nan,
                    excitation: ExcitationParams {
                        low_cut_hz: nan,
                        transient: nan,
                        drive_limit: nan,
                    },
                },
                follower: EnvelopeParams {
                    attack_ms: nan,
                    release_ms: -inf,
                },
                walk: RandomWalkParams { rate_hz: inf },
                ..mp(1.0, 1.0)
            },
        ];
        let mut input = noise(20_000);
        for (i, f) in input.iter_mut().enumerate() {
            match i % 4000 {
                0 => *f = [nan, inf],
                1 => *f = [-inf, 1e30],
                2 => *f = [1.0, 1.0],
                _ => {}
            }
        }
        // Full-scale input peak 1 after sanitising non-finite to 0 and clamping by the excitation.
        let bound = BANK_GAIN * VOICE_COUNT as f32 * 2.0;
        for p in &cases {
            let mut m = ModulatedResonator::new(FS, p);
            for f in &input {
                let y = m.tick(*f);
                assert!(y[0].is_finite() && y[1].is_finite(), "{p:?}");
                assert!(y[0].abs() <= bound && y[1].abs() <= bound, "{p:?} {y:?}");
            }
        }
    }

    /// Largest sample-to-sample change of the right channel over 20 ms after a base parameter
    /// step with both routes at full depth, driven by an 880 Hz tone (see the bank's test).
    fn step_jump(offset: usize) -> f32 {
        let tone = |n: usize| {
            let v = 0.5 * sinf(2.0 * core::f32::consts::PI * 880.0 * n as f32 / FS);
            [v, v]
        };
        let with = |damping: f32, spread: f32| ModulationParams {
            base: ResonatorParams {
                root_hz: 110.0,
                structure: 0.0,
                damping,
                spread,
                ..ResonatorParams::default()
            },
            ..mp(1.0, 1.0)
        };
        let mut m = ModulatedResonator::new(FS, &with(0.5, 1.0));
        let mut n = 0;
        let mut prev = 0.0;
        while n < 24_000 + offset {
            prev = m.tick(tone(n))[1];
            n += 1;
        }
        m.set_params(&with(0.2, 0.0));
        let mut worst = 0.0f32;
        for _ in 0..960 {
            let y = m.tick(tone(n))[1];
            worst = worst.max(fabsf(y - prev));
            prev = y;
            n += 1;
        }
        worst
    }

    #[test]
    fn parameter_step_does_not_click() {
        const JUMP_LIMIT: f32 = 0.08;
        let worst = (0..56).map(step_jump).fold(0.0f32, f32::max);
        assert!(worst < JUMP_LIMIT, "jump {worst}");
    }

    fn split_blocks(input: &[Frame], sizes: &[usize]) -> Vec<Frame> {
        let mut m = ModulatedResonator::new(FS, &mp(0.7, 0.7));
        let mut out = input.to_vec();
        let (mut at, mut k) = (0, 0);
        while at < out.len() {
            let end = (at + sizes[k % sizes.len()]).min(out.len());
            m.process_block(&mut out[at..end]);
            at = end;
            k += 1;
        }
        out
    }

    #[test]
    fn process_block_equals_repeated_tick() {
        let input = noise(5_000);
        let want = render(&mut ModulatedResonator::new(FS, &mp(0.7, 0.7)), &input);
        for sizes in [&[32][..], &[1], &[7, 33, 100], &[5_000]] {
            assert_eq!(bits(&split_blocks(&input, sizes)), bits(&want), "{sizes:?}");
        }
    }

    #[test]
    fn reset_restores_the_start_of_the_walk() {
        let input = noise(20_000);
        let mut m = ModulatedResonator::new(FS, &mp(0.7, 0.7));
        let want = render(&mut m, &input);
        let mut trace_a = Vec::new();
        m.reset();
        for f in &input {
            m.tick(*f);
            trace_a.push(m.effective_params().spread.to_bits());
        }
        m.reset();
        m.reset();
        let again = render(&mut m, &input);
        assert_eq!(bits(&again), bits(&want));
        let mut fresh = ModulatedResonator::new(FS, &mp(0.7, 0.7));
        let trace_b: Vec<u32> = input
            .iter()
            .map(|f| {
                fresh.tick(*f);
                fresh.effective_params().spread.to_bits()
            })
            .collect();
        assert_eq!(trace_a, trace_b);
    }
}
