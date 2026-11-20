//! A single tuned resonator voice: rings at a set pitch when excited, decays over a set time.
//!
//! # Formulation
//!
//! A damped rotation (the Gordon-Smith "magic circle" coupled oscillator with a pole radius).
//! The state `(x, y)` is updated every sample as
//!
//! ```text
//! x' = r * (cos(w) * x - sin(w) * y) + (1 - r) * in
//! y' = r * (sin(w) * x + cos(w) * y)
//! out = y'
//! ```
//!
//! with `w = 2π · pitch / fs` and `r = exp(-ln(1000) / (decay · fs))`, so `decay` is the time
//! for the ring to fall by 60 dB (T60). The rotation preserves the state norm, so each step is a
//! contraction by `r < 1` plus an input of at most `(1 - r) · |in|`; the state norm can therefore
//! never exceed the input peak, at any pitch up to Nyquist and while pitch and decay glide every
//! sample. Smoothing the coefficients of a biquad or state-variable resonator offers no such
//! guarantee (see the module `smooth`).
//!
//! The `(1 - r)` input scaling is what bounds the output; it also makes the impulse response of
//! a long decay quiet (about `1.4e-5` peak at 10 s). A bank of voices applies its own makeup
//! gain.

use crate::param::{ParamSpec, Taper};
use crate::processor::{Frame, Processor};
use crate::smooth::{sanitize, Smoother};
use libm::{expf as exp, logf as ln, sincosf};

/// Parameter table for [`ResonatorVoice`]; the only source of its ranges, tapers and defaults.
///
/// `pitch_hz` is the ring frequency; `decay_s` is the time for the ring to fall by 60 dB. The
/// defaults are a 110 Hz (A2) voice that rings for about a second.
pub const RESONATOR_VOICE_PARAMS: [ParamSpec; 2] = [
    ParamSpec {
        name: "pitch_hz",
        unit: "Hz",
        min: 20.0,
        max: 8_000.0,
        taper: Taper::Log,
        default: 110.0,
    },
    ParamSpec {
        name: "decay_s",
        unit: "s",
        min: 0.05,
        max: 10.0,
        taper: Taper::Log,
        default: 1.0,
    },
];

/// Smoothing time constant for the log-pitch and log-decay glides, in milliseconds.
const SMOOTH_MS: f32 = 10.0;

/// Lowest pitch the voice holds. A floor, not a musical limit: it keeps `ln(pitch)` defined.
const MIN_PITCH_HZ: f32 = 1.0;

/// Highest pitch as a fraction of the sample rate, just below Nyquist (0.5).
const MAX_PITCH_FRACTION: f32 = 0.49;

/// Shortest and longest decay the processor holds. Wider than the knob range so values just
/// outside it still behave, and a zero or negative decay becomes the shortest.
const MIN_DECAY_S: f32 = 0.001;
const MAX_DECAY_S: f32 = 60.0;

/// `ln(1000)`: a fall of 60 dB in amplitude.
const LN_1000: f32 = 6.907_755_3;

/// Parameters for [`ResonatorVoice`].
#[derive(Clone, Debug)]
pub struct ResonatorVoiceParams {
    /// Ring frequency in Hz. NaN becomes the default; values at or above Nyquist are held just
    /// below it.
    pub pitch_hz: f32,
    /// Time for the ring to fall by 60 dB, in seconds. NaN becomes the default; zero or negative
    /// becomes the shortest decay.
    pub decay_s: f32,
}

impl Default for ResonatorVoiceParams {
    fn default() -> Self {
        Self {
            pitch_hz: RESONATOR_VOICE_PARAMS[0].default,
            decay_s: RESONATOR_VOICE_PARAMS[1].default,
        }
    }
}

/// One tuned resonator: a damped rotation excited by its input.
///
/// The two input channels are averaged to one, and the same output goes to both channels. A
/// caller composing many voices uses [`tick_mono`](Self::tick_mono) and skips the duplication.
pub struct ResonatorVoice {
    sample_rate_hz: f32,
    /// Sanitised requested pitch (not yet clamped to Nyquist) and decay; the smoother targets
    /// derive from these and the sample rate, so `set_params` and `set_sample_rate` commute.
    pitch_hz: f32,
    decay_s: f32,
    ln_pitch: Smoother,
    ln_decay: Smoother,
    /// The smoothed pair the cached coefficients came from; NaN when invalid. Bit-compared, so a
    /// settled voice costs no `exp`, `sin` or `cos` calls.
    cached_ln_pitch: f32,
    cached_ln_decay: f32,
    cached_r: f32,
    cached_sin: f32,
    cached_cos: f32,
    x: f32,
    y: f32,
}

impl Default for ResonatorVoice {
    fn default() -> Self {
        let p = ResonatorVoiceParams::default();
        Self {
            sample_rate_hz: 48_000.0,
            pitch_hz: p.pitch_hz,
            decay_s: p.decay_s,
            ln_pitch: Smoother::new(ln(p.pitch_hz), SMOOTH_MS, 48_000.0),
            ln_decay: Smoother::new(ln(p.decay_s), SMOOTH_MS, 48_000.0),
            cached_ln_pitch: f32::NAN,
            cached_ln_decay: f32::NAN,
            cached_r: 0.0,
            cached_sin: 0.0,
            cached_cos: 1.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

impl Processor for ResonatorVoice {
    type Params = ResonatorVoiceParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 && hz.is_finite() {
            self.sample_rate_hz = hz;
            self.ln_pitch.update_sample_rate(SMOOTH_MS, hz);
            self.ln_decay.update_sample_rate(SMOOTH_MS, hz);
            // The coefficients depend on the sample rate even when the smoothed values do not
            // move, and the pitch ceiling moves with it.
            self.cached_ln_pitch = f32::NAN;
            self.recompute();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        let default = ResonatorVoiceParams::default();
        self.pitch_hz = if params.pitch_hz.is_nan() {
            default.pitch_hz
        } else {
            params.pitch_hz.max(MIN_PITCH_HZ)
        };
        self.decay_s = if params.decay_s.is_nan() {
            default.decay_s
        } else {
            params.decay_s.clamp(MIN_DECAY_S, MAX_DECAY_S)
        };
        self.recompute();
    }

    fn tick(&mut self, input: Frame) -> Frame {
        let mono = 0.5 * (sanitize(input[0]) + sanitize(input[1]));
        let out = self.tick_mono(mono);
        [out, out]
    }

    fn reset(&mut self) {
        self.ln_pitch.snap();
        self.ln_decay.snap();
        self.x = 0.0;
        self.y = 0.0;
    }
}

impl ResonatorVoice {
    /// A voice at `sample_rate_hz` with `params` already in force: the first sample rings at the
    /// requested pitch and decay, not ones glided in from the default.
    pub fn new(sample_rate_hz: f32, params: &ResonatorVoiceParams) -> Self {
        let mut v = Self::default();
        v.set_sample_rate(sample_rate_hz);
        v.set_params(params);
        v.reset();
        v
    }

    /// Process one mono sample. [`Processor::tick`] is this with the channels averaged in and
    /// the result written to both.
    pub fn tick_mono(&mut self, input: f32) -> f32 {
        let lp = self.ln_pitch.advance();
        let ld = self.ln_decay.advance();
        if lp.to_bits() != self.cached_ln_pitch.to_bits()
            || ld.to_bits() != self.cached_ln_decay.to_bits()
        {
            let pitch = exp(lp);
            let decay = exp(ld);
            let r = exp(-LN_1000 / (decay * self.sample_rate_hz));
            // Strictly below one even if the exponent underflows against 1.0.
            self.cached_r = r.min(1.0 - f32::EPSILON / 2.0);
            let (s, c) = sincosf(2.0 * core::f32::consts::PI * pitch / self.sample_rate_hz);
            self.cached_sin = s;
            self.cached_cos = c;
            self.cached_ln_pitch = lp;
            self.cached_ln_decay = ld;
        }
        let r = self.cached_r;
        let (x, y) = (self.x, self.y);
        self.x =
            sanitize(r * (self.cached_cos * x - self.cached_sin * y) + (1.0 - r) * sanitize(input));
        self.y = sanitize(r * (self.cached_sin * x + self.cached_cos * y));
        self.y
    }

    /// Set the smoother targets from the stored pitch, decay and sample rate. The pitch is
    /// clamped to `[MIN_PITCH_HZ, 0.49 · fs]` here, so a later sample-rate change re-clamps it.
    fn recompute(&mut self) {
        let ceiling = (MAX_PITCH_FRACTION * self.sample_rate_hz).max(MIN_PITCH_HZ);
        self.ln_pitch.set_target(ln(self.pitch_hz.min(ceiling)));
        self.ln_decay.set_target(ln(self.decay_s));
    }

    /// Jump the smoothed pitch and decay to their targets now, keeping the ring. Unlike
    /// [`Processor::reset`] this does not clear the state.
    pub fn snap_params(&mut self) {
        self.ln_pitch.snap();
        self.ln_decay.snap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libm::fabsf;

    const FS: f32 = 48_000.0;

    fn voice(pitch_hz: f32, decay_s: f32) -> ResonatorVoice {
        ResonatorVoice::new(FS, &ResonatorVoiceParams { pitch_hz, decay_s })
    }

    /// An impulse of amplitude 1 followed by `len - 1` zeros, rendered mono.
    fn impulse_response(v: &mut ResonatorVoice, len: usize) -> Vec<f32> {
        (0..len)
            .map(|n| v.tick_mono(if n == 0 { 1.0 } else { 0.0 }))
            .collect()
    }

    /// Frequency from positive-going zero crossings, interpolated linearly: crossings per
    /// elapsed time between the first and the last.
    fn measured_hz(y: &[f32]) -> f32 {
        let mut crossings = Vec::new();
        for n in 1..y.len() {
            if y[n - 1] < 0.0 && y[n] >= 0.0 {
                crossings.push(n as f32 - 1.0 + (-y[n - 1]) / (y[n] - y[n - 1]));
            }
        }
        assert!(
            crossings.len() >= 3,
            "too few crossings: {}",
            crossings.len()
        );
        (crossings.len() - 1) as f32 * FS / (crossings[crossings.len() - 1] - crossings[0])
    }

    #[test]
    fn default_params_match_the_table() {
        let p = ResonatorVoiceParams::default();
        assert_eq!(p.pitch_hz, RESONATOR_VOICE_PARAMS[0].default);
        assert_eq!(p.decay_s, RESONATOR_VOICE_PARAMS[1].default);
    }

    #[test]
    fn default_voice_rings_audibly_for_about_a_second() {
        let mut v = ResonatorVoice::new(FS, &ResonatorVoiceParams::default());
        let y = impulse_response(&mut v, 96_000);
        let peak = |a: usize, b: usize| y[a..b].iter().fold(0.0f32, |m, s| m.max(fabsf(*s)));
        let early = peak(0, 4_800);
        let at_t60 = peak(48_000 - 2_400, 48_000 + 2_400);
        // An impulse of 1 puts (1 - r), about 1.4e-4 at 1 s, into the state.
        assert!(early > 1e-4, "early peak {early}");
        // -60 dB at one decay time, within a factor of 2 of 1e-3 of the early peak.
        assert!(
            at_t60 < early * 2e-3 && at_t60 > early * 4e-4,
            "{early} {at_t60}"
        );
    }

    /// Pitch accuracy across the range: within 0.5 %.
    #[test]
    fn impulse_rings_at_the_set_pitch() {
        for pitch in [20.0, 110.0, 440.0, 2000.0, 8000.0] {
            let mut v = voice(pitch, 5.0);
            let y = impulse_response(&mut v, 48_000);
            let got = measured_hz(&y);
            assert!(
                fabsf(got - pitch) / pitch < 0.005,
                "pitch {pitch}: measured {got}"
            );
        }
    }

    /// Decay accuracy: a half and a whole T60 later the ring has fallen by -30 dB (+/- 0.5 dB)
    /// and -60 dB (+/- 0.5 dB). Peaks are taken over one 48-sample cycle of a 1 kHz ring, so
    /// both windows see the same phase grid.
    #[test]
    fn impulse_decays_at_the_set_decay_time() {
        for decay in [0.1, 1.0, 5.0] {
            let mut v = voice(1000.0, decay);
            let y = impulse_response(&mut v, (1.2 * decay * FS) as usize);
            let cycle_peak = |k: usize| {
                y[k * 48..(k + 1) * 48]
                    .iter()
                    .fold(0.0f32, |m, s| m.max(fabsf(*s)))
            };
            let k0 = 5;
            let cycles_per_t60 = decay * 1000.0;
            for fraction in [0.5, 1.0] {
                let k1 = k0 + (fraction * cycles_per_t60) as usize;
                let elapsed = (k1 - k0) as f32 / 1000.0;
                let want_db = -60.0 * elapsed / decay;
                let got_db = 20.0 * libm::log10f(cycle_peak(k1) / cycle_peak(k0));
                assert!(
                    fabsf(got_db - want_db) < 0.5,
                    "decay {decay} at {fraction}: {got_db} dB, want {want_db}"
                );
            }
        }
    }

    #[test]
    fn silence_in_gives_exact_silence_after_the_tail() {
        for decay in [0.05, 1.0, 10.0] {
            let mut v = voice(330.0, decay);
            v.tick_mono(1.0);
            let tail = (2.0 * decay * FS) as usize;
            let mut last = 1.0;
            for _ in 0..tail {
                last = v.tick_mono(0.0);
            }
            assert!(fabsf(last) < 1e-6, "decay {decay}: {last}");
        }
        // The exact-zero flush: far past the tail the state is exactly zero.
        let mut v = voice(330.0, 0.05);
        v.tick_mono(1.0);
        let mut last = 1.0;
        for _ in 0..200_000 {
            last = v.tick_mono(0.0);
        }
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_is_idempotent_and_equals_a_fresh_voice() {
        let p = ResonatorVoiceParams {
            pitch_hz: 523.0,
            decay_s: 0.7,
        };
        let drive = |v: &mut ResonatorVoice| {
            (0..2000)
                .map(|n| v.tick_mono(if n < 3 { 0.8 } else { 0.0 }).to_bits())
                .collect::<Vec<_>>()
        };
        let mut used = ResonatorVoice::new(FS, &p);
        for n in 0..5000 {
            used.tick_mono(libm::sinf(n as f32 * 0.3));
        }
        used.set_params(&ResonatorVoiceParams {
            pitch_hz: 3000.0,
            decay_s: 3.0,
        });
        used.tick_mono(1.0);
        used.set_params(&p);
        used.reset();
        used.reset();
        let a = drive(&mut used);
        let b = drive(&mut ResonatorVoice::new(FS, &p));
        assert_eq!(a, b);
    }

    #[test]
    fn process_block_equals_repeated_tick() {
        let p = ResonatorVoiceParams {
            pitch_hz: 880.0,
            decay_s: 0.4,
        };
        let input: Vec<Frame> = (0..1000)
            .map(|n| {
                [
                    libm::sinf(n as f32 * 0.07),
                    0.25 * libm::cosf(n as f32 * 0.11),
                ]
            })
            .collect();
        let mut by_block = input.clone();
        ResonatorVoice::new(FS, &p).process_block(&mut by_block);
        let mut t = ResonatorVoice::new(FS, &p);
        for (n, f) in input.iter().enumerate() {
            let o = t.tick(*f);
            assert_eq!(o[0].to_bits(), by_block[n][0].to_bits());
            assert_eq!(o[1].to_bits(), by_block[n][1].to_bits());
        }
    }

    #[test]
    fn both_channels_carry_the_same_ring() {
        let mut v = voice(440.0, 1.0);
        let o = v.tick([1.0, 0.0]);
        assert_eq!(o[0].to_bits(), o[1].to_bits());
        let o = v.tick([0.0, 0.0]);
        assert_eq!(o[0].to_bits(), o[1].to_bits());
        assert!(o[0] != 0.0);
    }

    #[test]
    fn hostile_params_stay_finite_and_bounded() {
        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let cases = [
            (nan, 1.0),
            (440.0, nan),
            (nan, nan),
            (440.0, 0.0),
            (440.0, -1.0),
            (440.0, -inf),
            (440.0, inf),
            (FS / 2.0, 1.0),
            (FS, 1.0),
            (3.0 * FS, 1.0),
            (inf, 1.0),
            (-inf, 1.0),
            (-5.0, 1.0),
            (0.0, 1.0),
            (f32::MAX, f32::MAX),
        ];
        for (pitch_hz, decay_s) in cases {
            let mut v = voice(110.0, 1.0);
            v.set_params(&ResonatorVoiceParams { pitch_hz, decay_s });
            for n in 0..20_000 {
                let x = if n % 97 < 40 { 1.0 } else { -1.0 };
                let y = v.tick_mono(x);
                assert!(
                    y.is_finite() && fabsf(y) <= 1.0 + 1e-5,
                    "{pitch_hz} {decay_s}: {y}"
                );
            }
        }
    }

    #[test]
    fn lower_sample_rate_reclamps_the_pitch() {
        let mut v = voice(20_000.0, 1.0);
        v.set_sample_rate(8_000.0);
        for n in 0..20_000 {
            let y = v.tick_mono(if n % 50 < 25 { 1.0 } else { -1.0 });
            assert!(y.is_finite() && fabsf(y) <= 1.0 + 1e-5, "{y}");
        }
    }

    #[test]
    fn worst_case_drive_stays_within_the_input_peak() {
        // Constant full-scale input at the slowest pitch and longest decay: the largest state a
        // contraction by r can build.
        let mut v = voice(20.0, 10.0);
        let mut peak = 0.0f32;
        for _ in 0..(12.0 * FS) as usize {
            peak = peak.max(fabsf(v.tick_mono(1.0)));
        }
        assert!(peak <= 1.0 + 1e-5, "peak {peak}");
    }

    #[test]
    fn rate_and_params_commute() {
        let p = ResonatorVoiceParams {
            pitch_hz: 3000.0,
            decay_s: 0.5,
        };
        let run = |rate_first: bool| {
            let mut v = ResonatorVoice::default();
            if rate_first {
                v.set_sample_rate(8000.0);
                v.set_params(&p);
            } else {
                v.set_params(&p);
                v.set_sample_rate(8000.0);
            }
            v.reset();
            (0..200)
                .map(|n| v.tick_mono(if n == 0 { 1.0 } else { 0.0 }).to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(run(true), run(false));
    }

    /// Ring at 110 Hz, step the pitch to 880 Hz, and look at the next 96 samples (2 ms). The
    /// instantaneous frequency of the output must not jump: the largest sample-to-sample change
    /// may be at most 3 times what the old tone does. Smoothed, the pitch has moved about 20 %
    /// by then; unsmoothed, the ring turns at the new pitch from the first sample.
    fn max_delta_after_pitch_step(snap: bool) -> (f32, f32) {
        let mut v = voice(110.0, 5.0);
        for n in 0..48_000 {
            v.tick_mono(libm::sinf(
                2.0 * core::f32::consts::PI * 110.0 * n as f32 / FS,
            ));
        }
        let mut prev = v.tick_mono(0.0);
        let mut before = 0.0f32;
        for _ in 0..436 {
            let y = v.tick_mono(0.0);
            before = before.max(fabsf(y - prev));
            prev = y;
        }
        v.set_params(&ResonatorVoiceParams {
            pitch_hz: 880.0,
            decay_s: 5.0,
        });
        if snap {
            v.snap_params();
        }
        let mut after = 0.0f32;
        for _ in 0..96 {
            let y = v.tick_mono(0.0);
            after = after.max(fabsf(y - prev));
            prev = y;
        }
        (before, after)
    }

    #[test]
    fn pitch_step_does_not_jump() {
        let (before, after) = max_delta_after_pitch_step(false);
        assert!(before > 0.0);
        assert!(after <= 3.0 * before, "smoothed {after} vs steady {before}");
    }

    #[test]
    fn negative_control_unsmoothed_pitch_step_jumps() {
        let (before, after) = max_delta_after_pitch_step(true);
        assert!(
            after > 3.0 * before,
            "unsmoothed {after} vs steady {before}"
        );
    }
}
