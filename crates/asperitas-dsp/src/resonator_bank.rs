//! The resonator effect: the excitation stage into a bank of tuned voices, spread across stereo.
//!
//! The tuning model, voice count and default voicing are the owner's decision recorded in
//! `backlog/decisions/decision-001 - M5-resonator-tuning-voices-and-pod-mapping.md`: a root note
//! plus a structure series, with the root as a plain input to the bank (so a pitch tracker can
//! later drive the same field), [`VOICE_COUNT`] = 8 voices, and a default voicing of A2 (110 Hz),
//! structure 0, damping 0.5 (about one second of ring), the gentle excitation preset and wide
//! spread. The page `docs/reference/resonator.md` documents the formulas below.
//!
//! # Signal flow
//!
//! The two input channels are averaged to one (`0.5 * (l + r)`), shaped by [`Excitation`], and
//! fed to every [`ResonatorVoice`]. Each voice output is scaled by [`BANK_GAIN`], by a smoothed
//! mute gain, and by an equal-power pan gain, and the voices are summed into left and right.
//!
//! # Tuning (structure)
//!
//! Voice `n` (1-based) rings at `root * n * sqrt(1 + B * (n^2 - 1))` with `B = structure *
//! STRETCH_MAX`: the stiff-string stretch of a harmonic series. Structure 0 is the pure harmonic
//! series, structure 1 an inharmonic, bell-like one. The `n^2 - 1` term leaves the root
//! unstretched. A partial above 0.49 of the sample rate is muted (its gain glides to zero)
//! rather than stacked at the pitch ceiling.
//!
//! # Damping
//!
//! Damping is how fast the bank decays and how bright the tail stays, and higher damping means a
//! shorter, darker ring. The root's T60 is `10 s * 0.01^damping` (10 s, 1 s, 0.1 s at damping 0,
//! 0.5, 1). Partial `n` rings for the root T60 divided by `1 + TILT * damping * (n - 1)`, so
//! higher partials die sooner the more the bank is damped.
//!
//! # Spread
//!
//! Voice `i` (0-based) sits at pan `(-1)^(i+1) * (i + 1) / 8`: alternating left and right, the
//! root nearly centred and the highest partial hard right. The effective pan is that times
//! `spread`; gains are equal power, `cos` and `sin` of `pi/4 * (pan + 1)`, computed so that pan
//! 0 gives the identical expression for both channels. Spread 0 therefore gives `L == R`
//! exactly. `spread` is smoothed.
//!
//! # Bound
//!
//! Each voice output never exceeds the peak of its input, an equal-power gain never exceeds 1,
//! and the excitation output never exceeds `min(drive_limit_max, 6 * input_peak)`, so
//!
//! ```text
//! |out| <= BANK_GAIN * VOICE_COUNT * min(2.0, 6 * input_peak)
//! ```
//!
//! at any params and while they glide. The worst case (input peak 1) is `2 * 8 * 2 = 32`. This
//! is a bound, not a level: broadband material excites a voice far less than a tone at its
//! pitch, which is why the gain sits above 1. [`BANK_GAIN`] is 2: a voice is unit gain at
//! resonance, and 2 puts the centre of the stereo field (which loses 3 dB to equal power) back
//! near the level of the input.

use crate::excitation::{Excitation, ExcitationParams};
use crate::param::{ParamSpec, Taper};
use crate::processor::{Frame, Processor};
use crate::resonator::{ResonatorVoice, ResonatorVoiceParams};
use crate::smooth::{sanitize, Smoother};
use libm::{powf, sincosf, sqrtf};

/// Number of voices in the bank.
pub const VOICE_COUNT: usize = 8;

/// Makeup gain applied to the summed voices; see the module docs for the bound.
pub const BANK_GAIN: f32 = 2.0;

/// Parameter table for the four bank parameters of [`ResonatorParams`]; the only source of their
/// ranges, tapers and defaults. The excitation parameters are described by
/// [`EXCITATION_PARAMS`](crate::EXCITATION_PARAMS).
///
/// - `root_hz`: the pitch of voice 1, 20 Hz to 2 kHz, default 110 (A2).
/// - `structure`: 0 is the harmonic series, 1 the most stretched.
/// - `damping`: 0 rings longest and brightest, 1 shortest and darkest; 0.5 is about 1 s.
/// - `spread`: 0 is mono (L equals R), 1 is the widest placement.
pub const RESONATOR_PARAMS: [ParamSpec; 4] = [
    ParamSpec {
        name: "root_hz",
        unit: "Hz",
        min: 20.0,
        max: 2_000.0,
        taper: Taper::Log,
        default: 110.0,
    },
    ParamSpec {
        name: "structure",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 0.0,
    },
    ParamSpec {
        name: "damping",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 0.5,
    },
    ParamSpec {
        name: "spread",
        unit: "",
        min: 0.0,
        max: 1.0,
        taper: Taper::Linear,
        default: 1.0,
    },
];

/// Stiffness coefficient at structure 1: partial 8 sits at about 12.0 times the root instead of 8.
const STRETCH_MAX: f32 = 0.02;

/// Root T60 at damping 0, and the factor it falls by across the damping range (0.1 s at 1).
const ROOT_DECAY_MAX_S: f32 = 10.0;
const ROOT_DECAY_RANGE: f32 = 0.01;

/// How strongly damping shortens higher partials relative to the root.
const TILT: f32 = 0.5;

/// Highest ring pitch as a fraction of the sample rate; a partial above it is muted.
pub const MAX_PITCH_FRACTION: f32 = 0.49;

/// Highest root the bank holds. Far above the knob range; it keeps pitches finite.
const MAX_ROOT_HZ: f32 = 20_000.0;

/// Smoothing time constant for spread and the Nyquist mute gains.
const SMOOTH_MS: f32 = 10.0;

/// Parameters for [`Resonator`].
#[derive(Clone, Debug, PartialEq)]
pub struct ResonatorParams {
    /// Pitch of voice 1 in Hz. NaN becomes the default; clamped to 1 Hz - 20 kHz.
    pub root_hz: f32,
    /// Harmonic (0) to stretched (1) series. NaN becomes the default; clamped to 0..1.
    pub structure: f32,
    /// 0 rings longest, 1 shortest. NaN becomes the default; clamped to 0..1.
    pub damping: f32,
    /// 0 mono, 1 widest. NaN becomes the default; clamped to 0..1.
    pub spread: f32,
    /// The excitation stage in front of the bank.
    pub excitation: ExcitationParams,
}

impl Default for ResonatorParams {
    fn default() -> Self {
        Self {
            root_hz: RESONATOR_PARAMS[0].default,
            structure: RESONATOR_PARAMS[1].default,
            damping: RESONATOR_PARAMS[2].default,
            spread: RESONATOR_PARAMS[3].default,
            excitation: ExcitationParams::default(),
        }
    }
}

impl ResonatorParams {
    /// The root voice's T60 in seconds at this damping (sanitised first). Every other voice rings
    /// no longer than this.
    pub fn root_decay_s(&self) -> f32 {
        root_decay_s(clean(self.damping, &RESONATOR_PARAMS[2], 0.0, 1.0))
    }
}

fn root_decay_s(damping: f32) -> f32 {
    ROOT_DECAY_MAX_S * powf(ROOT_DECAY_RANGE, damping)
}

/// NaN becomes the spec's default and everything else clamps to `[lo, hi]`.
fn clean(v: f32, spec: &ParamSpec, lo: f32, hi: f32) -> f32 {
    if v.is_nan() {
        spec.default
    } else {
        v.clamp(lo, hi)
    }
}

/// Pan position of voice `i` before spread, in `[-1, 1]`.
fn base_pan(i: usize) -> f32 {
    let magnitude = (i + 1) as f32 / VOICE_COUNT as f32;
    if i.is_multiple_of(2) {
        -magnitude
    } else {
        magnitude
    }
}

/// Left and right equal-power gains for a pan in `[-1, 1]`: `cos` and `sin` of `pi/4 * (pan + 1)`
/// written through `cos(t) -/+ sin(t)` with `t = pi/4 * pan`, so that pan 0 evaluates the same
/// expression for both channels.
fn pan_gains(pan: f32) -> (f32, f32) {
    let (s, c) = sincosf(core::f32::consts::FRAC_PI_4 * pan);
    (
        core::f32::consts::FRAC_1_SQRT_2 * (c - s),
        core::f32::consts::FRAC_1_SQRT_2 * (c + s),
    )
}

/// The resonator effect. See the module docs.
pub struct Resonator {
    sample_rate_hz: f32,
    /// Sanitised requested values; voice targets derive from these and the sample rate, so
    /// `set_params` and `set_sample_rate` commute.
    root_hz: f32,
    structure: f32,
    damping: f32,
    excitation: Excitation,
    voices: [ResonatorVoice; VOICE_COUNT],
    /// Smoothed 0..1 gain per voice: 0 for a partial above the pitch ceiling.
    mute: [Smoother; VOICE_COUNT],
    spread: Smoother,
    /// The smoothed spread the cached pan gains came from; NaN when stale.
    cached_spread: f32,
    left: [f32; VOICE_COUNT],
    right: [f32; VOICE_COUNT],
}

impl Default for Resonator {
    fn default() -> Self {
        let p = ResonatorParams::default();
        let mut r = Self {
            sample_rate_hz: 48_000.0,
            root_hz: p.root_hz,
            structure: p.structure,
            damping: p.damping,
            excitation: Excitation::default(),
            voices: core::array::from_fn(|_| ResonatorVoice::default()),
            mute: core::array::from_fn(|_| Smoother::new(1.0, SMOOTH_MS, 48_000.0)),
            spread: Smoother::new(p.spread, SMOOTH_MS, 48_000.0),
            cached_spread: f32::NAN,
            left: [0.0; VOICE_COUNT],
            right: [0.0; VOICE_COUNT],
        };
        r.retarget();
        r.snap_params();
        r
    }
}

impl Resonator {
    /// A bank at `sample_rate_hz` with `params` already in force, with no ramp from the default.
    pub fn new(sample_rate_hz: f32, params: &ResonatorParams) -> Self {
        let mut r = Self::default();
        r.set_sample_rate(sample_rate_hz);
        r.set_params(params);
        r.reset();
        r
    }

    /// Jump every smoothed parameter to its target now, keeping the ring and the signal state.
    pub fn snap_params(&mut self) {
        self.excitation.snap_params();
        for v in &mut self.voices {
            v.snap_params();
        }
        for m in &mut self.mute {
            m.snap();
        }
        self.spread.snap();
    }

    /// The ring pitch in Hz of voice `i` (0-based) for a root and structure, before the pitch
    /// ceiling ([`MAX_PITCH_FRACTION`]).
    pub fn partial_hz(root_hz: f32, structure: f32, i: usize) -> f32 {
        let n = (i + 1) as f32;
        let stiffness = structure * STRETCH_MAX;
        root_hz * n * sqrtf(1.0 + stiffness * (n * n - 1.0))
    }

    /// Set every voice's target from the stored root, structure, damping and sample rate.
    fn retarget(&mut self) {
        let ceiling = MAX_PITCH_FRACTION * self.sample_rate_hz;
        let root_decay = root_decay_s(self.damping);
        for i in 0..VOICE_COUNT {
            let pitch = Self::partial_hz(self.root_hz, self.structure, i);
            let tilt = 1.0 + TILT * self.damping * i as f32;
            self.voices[i].set_params(&ResonatorVoiceParams {
                pitch_hz: pitch,
                decay_s: root_decay / tilt,
            });
            self.mute[i].set_target(if pitch <= ceiling { 1.0 } else { 0.0 });
        }
    }

    /// Recompute the per-voice pan gains from a smoothed spread.
    fn update_pan_gains(&mut self, spread: f32) {
        for i in 0..VOICE_COUNT {
            let (l, r) = pan_gains(base_pan(i) * spread);
            self.left[i] = l;
            self.right[i] = r;
        }
        self.cached_spread = spread;
    }
}

impl Processor for Resonator {
    type Params = ResonatorParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 && hz.is_finite() {
            self.sample_rate_hz = hz;
            self.excitation.set_sample_rate(hz);
            for v in &mut self.voices {
                v.set_sample_rate(hz);
            }
            for m in &mut self.mute {
                m.update_sample_rate(SMOOTH_MS, hz);
            }
            self.spread.update_sample_rate(SMOOTH_MS, hz);
            self.retarget();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.root_hz = clean(params.root_hz, &RESONATOR_PARAMS[0], 1.0, MAX_ROOT_HZ);
        self.structure = clean(params.structure, &RESONATOR_PARAMS[1], 0.0, 1.0);
        self.damping = clean(params.damping, &RESONATOR_PARAMS[2], 0.0, 1.0);
        self.spread
            .set_target(clean(params.spread, &RESONATOR_PARAMS[3], 0.0, 1.0));
        self.excitation.set_params(&params.excitation);
        self.retarget();
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let mono = 0.5 * (sanitize(input[0]) + sanitize(input[1]));
        let excited = self.excitation.tick([mono, mono])[0];

        let spread = self.spread.advance();
        if spread.to_bits() != self.cached_spread.to_bits() {
            self.update_pan_gains(spread);
        }

        let (mut l, mut r) = (0.0, 0.0);
        for i in 0..VOICE_COUNT {
            let v = BANK_GAIN * self.mute[i].advance() * self.voices[i].tick_mono(excited);
            l += v * self.left[i];
            r += v * self.right[i];
        }
        [sanitize(l), sanitize(r)]
    }

    fn reset(&mut self) {
        self.excitation.reset();
        for v in &mut self.voices {
            v.reset();
        }
        self.snap_params();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libm::{fabsf, log10f, sinf};

    const FS: f32 = 48_000.0;

    fn bank(p: &ResonatorParams) -> Resonator {
        Resonator::new(FS, p)
    }

    fn impulse(b: &mut Resonator, len: usize) -> Vec<Frame> {
        (0..len)
            .map(|n| b.tick(if n == 0 { [1.0, 1.0] } else { [0.0, 0.0] }))
            .collect()
    }

    /// Energy of `x` at `hz`, by a Goertzel-style single-bin DFT (squared magnitude).
    fn bin_energy(x: &[f32], hz: f32) -> f32 {
        let w = 2.0 * core::f32::consts::PI * hz / FS;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, v) in x.iter().enumerate() {
            let (s, c) = sincosf(w * n as f32);
            re += *v as f64 * c as f64;
            im += *v as f64 * s as f64;
        }
        (re * re + im * im) as f32
    }

    fn channel(y: &[Frame], c: usize) -> Vec<f32> {
        y.iter().map(|f| f[c]).collect()
    }

    fn params(root_hz: f32, structure: f32, damping: f32, spread: f32) -> ResonatorParams {
        ResonatorParams {
            root_hz,
            structure,
            damping,
            spread,
            ..ResonatorParams::default()
        }
    }

    #[test]
    fn default_params_match_the_table_and_the_decision() {
        let p = ResonatorParams::default();
        assert_eq!(p.root_hz, 110.0);
        assert_eq!(p.structure, 0.0);
        assert_eq!(p.damping, 0.5);
        assert_eq!(p.spread, 1.0);
        assert_eq!(p.excitation, ExcitationParams::default());
        assert_eq!(VOICE_COUNT, 8);
        assert!(fabsf(p.root_decay_s() - 1.0) < 1e-4, "{}", p.root_decay_s());
    }

    /// With structure 0, each partial `n * 110` Hz holds far more energy than a point a quarter
    /// of the way to its neighbour, and the stretched series moves each partial to the
    /// documented pitch (within 0.5 %, found by scanning bins around it).
    #[test]
    fn impulse_rings_at_the_tuned_pitches() {
        for structure in [0.0, 1.0] {
            let mut b = bank(&params(110.0, structure, 0.0, 0.0));
            let y = channel(&impulse(&mut b, 48_000), 0);
            for i in 0..VOICE_COUNT {
                let want = Resonator::partial_hz(110.0, structure, i);
                // Scan +-3 % in 0.1 % steps for the strongest bin.
                let (mut best, mut best_e) = (0.0f32, 0.0f32);
                for k in -30..=30 {
                    let hz = want * (1.0 + k as f32 * 0.001);
                    let e = bin_energy(&y, hz);
                    if e > best_e {
                        best_e = e;
                        best = hz;
                    }
                }
                assert!(
                    fabsf(best - want) / want < 0.005,
                    "structure {structure} partial {i}: peak {best} want {want}"
                );
                // Well away from the partial there is much less energy.
                let off = bin_energy(&y, want * 1.15);
                assert!(best_e > 10.0 * off, "partial {i}: {best_e} vs off {off}");
            }
        }
        // The documented stretch: partial 8 at structure 1 is about 12 times the root.
        let ratio = Resonator::partial_hz(1.0, 1.0, 7) / 8.0;
        assert!(fabsf(ratio - 1.5) < 0.01, "{ratio}");
    }

    /// Rendered energy over the second half-second, relative to the first 50 ms.
    fn tail_db(damping: f32) -> f32 {
        let mut b = bank(&params(110.0, 0.0, damping, 0.0));
        let y = channel(&impulse(&mut b, 48_000), 0);
        let energy = |a: usize, c: usize| y[a..c].iter().map(|v| v * v).sum::<f32>();
        10.0 * log10f(energy(24_000, 48_000) / energy(0, 2_400))
    }

    /// Higher damping rings shorter: the tail is lower relative to the attack. Swapping the two
    /// settings flips the comparison (the negative control).
    #[test]
    fn higher_damping_shortens_the_tail() {
        let (loose, tight) = (tail_db(0.2), tail_db(0.8));
        assert!(tight < loose - 10.0, "tight {tight} dB, loose {loose} dB");
        let (swapped_loose, swapped_tight) = (tail_db(0.8), tail_db(0.2));
        assert!(swapped_tight >= swapped_loose - 10.0);
    }

    /// Damping 0.5 gives about a one second root ring: -60 dB at one second.
    #[test]
    fn default_root_rings_for_about_a_second() {
        let mut v = ResonatorVoice::new(
            FS,
            &ResonatorVoiceParams {
                pitch_hz: 110.0,
                decay_s: ResonatorParams::default().root_decay_s(),
            },
        );
        let y: Vec<f32> = (0..96_000)
            .map(|n| v.tick_mono(if n == 0 { 1.0 } else { 0.0 }))
            .collect();
        let peak = |a: usize, b: usize| y[a..b].iter().fold(0.0f32, |m, s| m.max(fabsf(*s)));
        let ratio = peak(48_000 - 600, 48_000 + 600) / peak(0, 1_200);
        assert!(ratio < 2e-3 && ratio > 4e-4, "{ratio}");
    }

    /// Energy of the left and right channel at the highest partial (hard right at full spread).
    fn lr_energy_at_partial_8(spread: f32) -> (f32, f32) {
        let mut b = bank(&params(110.0, 0.0, 0.0, spread));
        let y = impulse(&mut b, 24_000);
        (
            bin_energy(&channel(&y, 0), 880.0),
            bin_energy(&channel(&y, 1), 880.0),
        )
    }

    #[test]
    fn spread_zero_is_mono_and_full_spread_separates_voices() {
        let mut b = bank(&params(110.0, 0.0, 0.3, 0.0));
        for f in impulse(&mut b, 24_000) {
            assert_eq!(f[0].to_bits(), f[1].to_bits());
        }

        // Documented: voice 8 is hard right at spread 1, so the left holds under 1e-3 of the
        // right's energy there.
        let separated = |(l, r): (f32, f32)| l < 1e-3 * r;
        assert!(separated(lr_energy_at_partial_8(1.0)));
        // Negative control: at spread 0 the same assertion fails, the channels are equal.
        assert!(!separated(lr_energy_at_partial_8(0.0)));
        let (l, r) = lr_energy_at_partial_8(0.0);
        assert_eq!(l.to_bits(), r.to_bits());
    }

    #[test]
    fn pan_gains_are_equal_power_and_at_most_one() {
        for k in -8..=8 {
            let (l, r) = pan_gains(k as f32 / 8.0);
            assert!(fabsf(l * l + r * r - 1.0) < 1e-5);
            assert!(l <= 1.0 + 1e-6 && r <= 1.0 + 1e-6);
        }
    }

    #[test]
    fn partials_above_the_pitch_ceiling_are_muted() {
        // Root 5 kHz: partials 2.. are above 0.49 * 48k = 23.5 kHz only from n = 5.
        let mut b = bank(&params(5_000.0, 0.0, 0.0, 0.0));
        let y = channel(&impulse(&mut b, 24_000), 0);
        let at_ceiling = bin_energy(&y, 0.49 * FS);
        let at_root = bin_energy(&y, 5_000.0);
        assert!(at_ceiling < 1e-3 * at_root, "{at_ceiling} {at_root}");
        assert!(y.iter().all(|v| v.is_finite()));
    }

    fn noise(len: usize) -> Vec<Frame> {
        let mut s = 12_345u32;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = (s >> 8) as f32 / (1 << 23) as f32 - 1.0;
                [0.5 * v, -0.5 * v]
            })
            .collect()
    }

    #[test]
    fn silence_in_gives_silence_out_after_the_tail() {
        let p = params(110.0, 0.5, 0.8, 1.0);
        let mut b = bank(&p);
        for f in noise(4_800) {
            b.tick(f);
        }
        let tail = (2.5 * p.root_decay_s() * FS) as usize + 10_000;
        let mut last = [1.0, 1.0];
        for _ in 0..tail {
            last = b.tick([0.0, 0.0]);
        }
        assert_eq!(last, [0.0, 0.0]);
    }

    #[test]
    fn reset_is_idempotent_and_deterministic() {
        let p = params(220.0, 0.4, 0.3, 0.7);
        let probe = noise(2_000);
        let render = |b: &mut Resonator| probe.iter().map(|f| b.tick(*f)).collect::<Vec<_>>();

        let mut fresh = bank(&p);
        let want = render(&mut fresh);

        let mut used = bank(&params(55.0, 0.9, 0.9, 0.0));
        for f in noise(5_000) {
            used.tick(f);
        }
        used.set_params(&p);
        used.reset();
        used.reset();
        assert_eq!(render(&mut used), want);
    }

    #[test]
    fn process_block_equals_repeated_tick() {
        let p = params(150.0, 0.6, 0.2, 0.8);
        let input = noise(1_000);
        let mut block = input.clone();
        bank(&p).process_block(&mut block);
        let mut t = bank(&p);
        let ticked: Vec<Frame> = input.iter().map(|f| t.tick(*f)).collect();
        assert_eq!(block, ticked);
    }

    /// Largest sample-to-sample change of the right channel over 20 ms after a parameter step,
    /// driving the bank with a steady 880 Hz tone (voice 8's pitch, hard right at full spread,
    /// so the right channel carries the ring). The step lands `offset` samples into the ring's
    /// cycle, about 55 samples long, so a click is not hidden by landing on a zero crossing.
    fn step_jump(snap: bool, offset: usize) -> f32 {
        let tone = |n: usize| {
            let v = 0.5 * sinf(2.0 * core::f32::consts::PI * 880.0 * n as f32 / FS);
            [v, v]
        };
        let mut b = bank(&params(110.0, 0.0, 0.5, 1.0));
        let mut n = 0;
        let mut prev = 0.0;
        while n < 24_000 + offset {
            prev = b.tick(tone(n))[1];
            n += 1;
        }
        b.set_params(&params(110.0, 0.0, 0.2, 0.0));
        if snap {
            b.snap_params();
        }
        let mut worst = 0.0f32;
        for _ in 0..960 {
            let y = b.tick(tone(n))[1];
            worst = worst.max(fabsf(y - prev));
            prev = y;
            n += 1;
        }
        worst
    }

    /// A step in spread and damping moves the output at most JUMP_LIMIT per sample at any phase
    /// (the ring itself moves about 0.06 per sample); snapping the smoothers instead produces a
    /// clearly larger jump at some phase.
    #[test]
    fn parameter_step_does_not_click() {
        const JUMP_LIMIT: f32 = 0.08;
        let smooth = (0..56).map(|o| step_jump(false, o)).fold(0.0f32, f32::max);
        assert!(smooth < JUMP_LIMIT, "smoothed jump {smooth}");
        let snapped = (0..56).map(|o| step_jump(true, o)).fold(0.0f32, f32::max);
        assert!(
            snapped > 1.5 * JUMP_LIMIT,
            "snapped {snapped} vs smoothed {smooth}"
        );
    }

    #[test]
    fn hostile_params_are_sanitised() {
        let nan = f32::NAN;
        let mut b = bank(&ResonatorParams {
            root_hz: nan,
            structure: f32::INFINITY,
            damping: -f32::INFINITY,
            spread: nan,
            excitation: ExcitationParams {
                low_cut_hz: nan,
                transient: nan,
                drive_limit: nan,
            },
        });
        for f in noise(2_000) {
            let y = b.tick(f);
            assert!(y[0].is_finite() && y[1].is_finite());
        }
    }
}
