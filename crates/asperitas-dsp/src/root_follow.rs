//! Root-follow stage: turns a pitch estimate into the root the resonator bank is tuned to.
//!
//! The behaviour is the owner's decision recorded in
//! `backlog/decisions/decision-002 - M6-modulation-routing-pod-mapping-and-tracked-root.md`
//! section 3, with every value a named constant here. The page `docs/reference/resonator.md`
//! documents the stage and the latency it adds.
//!
//! The stage is a pure control-rate function of the estimate and confidence of a
//! `PitchTracker` (passed in by the caller, so this module does not need the `pitch` feature),
//! the manual root and a transposition offset. The caller runs [`RootFollow::process_block`] once
//! per [`BLOCK_FRAMES`]-frame block and hands the result to the
//! bank as `root_hz`. The bank is unchanged.
//!
//! # Behaviour
//!
//! - Tracking off: the manual root is returned with its bits untouched, so the output is
//!   bit-identical to a bank driven by the manual root.
//! - Tracking on, confidence at or above [`CONFIDENCE_THRESHOLD`] and a finite positive
//!   estimate: the target is the estimate snapped to the nearest equal-tempered semitone
//!   (A4 = 440 Hz), [`SNAP_SEMITONES`].
//! - Tracking on, anything else (low confidence, unvoiced, NaN): the last target is held
//!   ([`LOW_CONFIDENCE_HOLDS`]). Before the first voiced estimate the target is the manual root.
//! - The root glides to the target as a one-pole in the semitone (log-frequency) domain with time
//!   constant [`GLIDE_MS`], advanced once per block, so the glide is the same for any sample rate
//!   up to the block quantisation. Switching tracking on starts the glide at the manual root.
//! - The transposition offset (semitones and cents, from the encoder) is added to the glided
//!   value, and the result is clamped to the `root_hz` range of
//!   [`RESONATOR_PARAMS`]. The output is always finite.

use crate::asperitas::BLOCK_FRAMES;
use crate::param::{ParamSpec, Taper};
use crate::resonator_bank::RESONATOR_PARAMS;
use crate::smooth::clean;
use libm::{exp2f, expf, log2f, roundf};

/// Confidence at or above which an estimate becomes the target. Equal to the pitch tracker's own
/// default clarity threshold.
pub const CONFIDENCE_THRESHOLD: f32 = 0.8;

/// Glide time constant in milliseconds.
pub const GLIDE_MS: f32 = 20.0;

/// Whether the target is snapped to the nearest semitone.
pub const SNAP_SEMITONES: bool = true;

/// Whether a low-confidence or unvoiced input holds the last target (the alternative, returning
/// to the manual root, jumps in pitch).
pub const LOW_CONFIDENCE_HOLDS: bool = true;

/// Parameter table for the transposition offset of [`RootFollowParams`]; the only source of its
/// ranges. The manual root's range is `RESONATOR_PARAMS[0]`.
pub const ROOT_FOLLOW_PARAMS: [ParamSpec; 2] = [
    ParamSpec {
        name: "offset_semitones",
        unit: "st",
        min: -48.0,
        max: 48.0,
        taper: Taper::Linear,
        default: 0.0,
    },
    ParamSpec {
        name: "offset_cents",
        unit: "cents",
        min: -100.0,
        max: 100.0,
        taper: Taper::Linear,
        default: 0.0,
    },
];

/// Parameters for [`RootFollow`].
#[derive(Clone, Debug, PartialEq)]
pub struct RootFollowParams {
    /// `false` returns `manual_root_hz` untouched; `true` follows the pitch estimate.
    pub tracking: bool,
    /// The manual root in Hz. Returned as is when tracking is off; the start of the glide when
    /// tracking is switched on and the target before the first voiced estimate.
    pub manual_root_hz: f32,
    /// Transposition added to the tracked root, whole semitones, -48..48. NaN falls back to 0.
    pub offset_semitones: f32,
    /// Transposition added to the tracked root, cents, -100..100. NaN falls back to 0.
    pub offset_cents: f32,
}

impl Default for RootFollowParams {
    fn default() -> Self {
        Self {
            tracking: false,
            manual_root_hz: RESONATOR_PARAMS[0].default,
            offset_semitones: ROOT_FOLLOW_PARAMS[0].default,
            offset_cents: ROOT_FOLLOW_PARAMS[1].default,
        }
    }
}

/// Semitones (MIDI note number, A4 = 440 Hz = 69) of a frequency.
fn to_semitones(hz: f32) -> f32 {
    69.0 + 12.0 * log2f(hz / 440.0)
}

fn from_semitones(st: f32) -> f32 {
    440.0 * exp2f((st - 69.0) / 12.0)
}

/// The root-follow stage. See the module docs.
pub struct RootFollow {
    params: RootFollowParams,
    /// Per-block glide coefficient.
    alpha: f32,
    /// Whether the stage was tracking at the last block; a switch on restarts the glide.
    was_tracking: bool,
    /// Held target in semitones.
    target: f32,
    /// Glided value in semitones, before the offset.
    glided: f32,
}

impl RootFollow {
    pub fn new(sample_rate_hz: f32) -> Self {
        let mut s = Self {
            params: RootFollowParams::default(),
            alpha: 1.0,
            was_tracking: false,
            target: 0.0,
            glided: 0.0,
        };
        s.set_sample_rate(sample_rate_hz);
        s
    }

    /// Set the sample rate, which sets the glide coefficient per block. A non-positive or
    /// non-finite rate is ignored.
    pub fn set_sample_rate(&mut self, hz: f32) {
        if hz > 0.0 && hz.is_finite() {
            let block_s = BLOCK_FRAMES as f32 / hz;
            self.alpha = 1.0 - expf(-block_s / (GLIDE_MS * 0.001));
        }
    }

    pub fn set_params(&mut self, params: &RootFollowParams) {
        self.params = params.clone();
    }

    /// Forget the target and glide; the next tracked block restarts from the manual root.
    pub fn reset(&mut self) {
        self.was_tracking = false;
        self.target = 0.0;
        self.glided = 0.0;
    }

    /// Advance one block and return the root in Hz for the bank. `estimate_hz` and `confidence`
    /// are the tracker's; they are ignored when tracking is off. Never NaN, and with tracking
    /// on always within the `root_hz` range of [`RESONATOR_PARAMS`].
    pub fn process_block(&mut self, estimate_hz: f32, confidence: f32) -> f32 {
        if !self.params.tracking {
            self.was_tracking = false;
            return self.params.manual_root_hz;
        }
        let spec = &RESONATOR_PARAMS[0];
        if !self.was_tracking {
            let start = to_semitones(clean(self.params.manual_root_hz, spec));
            self.target = start;
            self.glided = start;
            self.was_tracking = true;
        }
        let voiced =
            confidence >= CONFIDENCE_THRESHOLD && estimate_hz.is_finite() && estimate_hz > 0.0;
        if voiced {
            let st = to_semitones(estimate_hz);
            self.target = if SNAP_SEMITONES { roundf(st) } else { st };
        }
        self.glided += self.alpha * (self.target - self.glided);
        let offset = clean(self.params.offset_semitones, &ROOT_FOLLOW_PARAMS[0])
            + 0.01 * clean(self.params.offset_cents, &ROOT_FOLLOW_PARAMS[1]);
        let hz = from_semitones(self.glided + offset);
        if hz.is_nan() {
            spec.default
        } else {
            hz.clamp(spec.min, spec.max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn on(manual: f32) -> RootFollow {
        let mut r = RootFollow::new(RATE);
        r.set_params(&RootFollowParams {
            tracking: true,
            manual_root_hz: manual,
            ..RootFollowParams::default()
        });
        r
    }

    #[test]
    fn tracking_off_returns_the_manual_root_bits() {
        let mut r = RootFollow::new(RATE);
        for manual in [110.0, 0.0, -5.0, f32::NAN, f32::INFINITY, 12345.678] {
            r.set_params(&RootFollowParams {
                manual_root_hz: manual,
                offset_semitones: 7.0,
                ..RootFollowParams::default()
            });
            for (e, c) in [(440.0, 0.99), (f32::NAN, f32::NAN), (0.0, 0.0)] {
                assert_eq!(r.process_block(e, c).to_bits(), manual.to_bits());
            }
        }
    }

    #[test]
    fn negative_control_tracking_on_differs() {
        let mut r = on(110.0);
        let mut out = 0.0;
        for _ in 0..400 {
            out = r.process_block(440.0, 0.95);
        }
        assert_ne!(out.to_bits(), 110.0f32.to_bits());
        assert!((out - 440.0).abs() < 0.5, "{out}");
    }

    #[test]
    fn low_confidence_and_unvoiced_hold_the_last_root() {
        let mut r = on(110.0);
        for _ in 0..400 {
            r.process_block(220.0, 0.9);
        }
        let held = r.process_block(220.0, 0.9);
        for (e, c) in [
            (330.0, 0.5),
            (0.0, 0.0),
            (f32::NAN, 0.99),
            (330.0, f32::NAN),
        ] {
            for _ in 0..400 {
                assert_eq!(r.process_block(e, c).to_bits(), held.to_bits());
            }
        }
    }

    #[test]
    fn before_the_first_voiced_estimate_the_root_is_the_manual_root() {
        let mut r = on(110.0);
        for _ in 0..50 {
            let out = r.process_block(0.0, 0.0);
            assert!((out - 110.0).abs() < 1e-3, "{out}");
        }
    }

    #[test]
    fn snaps_to_the_nearest_semitone() {
        let mut r = on(110.0);
        let mut out = 0.0;
        // 30 cents sharp of A3 snaps to 220 Hz; 60 cents sharp snaps to A#3.
        for _ in 0..1000 {
            out = r.process_block(220.0 * exp2f(0.30 / 12.0), 0.9);
        }
        assert!((out - 220.0).abs() < 0.01, "{out}");
        for _ in 0..1000 {
            out = r.process_block(220.0 * exp2f(0.60 / 12.0), 0.9);
        }
        assert!((out - 220.0 * exp2f(1.0 / 12.0)).abs() < 0.01, "{out}");
    }

    #[test]
    fn glides_without_jumping() {
        let mut r = on(110.0);
        for _ in 0..1000 {
            r.process_block(110.0, 0.9);
        }
        let mut prev = to_semitones(r.process_block(110.0, 0.9));
        let mut first_step = None;
        let mut steps = 0;
        // An octave up: 12 semitones. Each block moves at most alpha * remaining distance.
        loop {
            let st = to_semitones(r.process_block(220.0, 0.9));
            let step = st - prev;
            assert!(step >= 0.0 && step <= r.alpha * 12.0 + 1e-3, "{step}");
            first_step.get_or_insert(step);
            prev = st;
            steps += 1;
            if (st - 57.0).abs() < 0.05 {
                break;
            }
            assert!(steps < 1000);
        }
        // Reaching within 0.05 semitones takes several time constants, not one block.
        assert!(steps > 10, "{steps}");
    }

    #[test]
    fn offset_transposes_the_tracked_root() {
        let mut r = RootFollow::new(RATE);
        r.set_params(&RootFollowParams {
            tracking: true,
            manual_root_hz: 220.0,
            offset_semitones: 12.0,
            offset_cents: 0.0,
        });
        let mut out = 0.0;
        for _ in 0..500 {
            out = r.process_block(220.0, 0.9);
        }
        assert!((out - 440.0).abs() < 0.01, "{out}");
    }

    #[test]
    fn output_is_finite_and_in_range_for_hostile_input() {
        let spec = &RESONATOR_PARAMS[0];
        let hostile = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -1.0,
            1e-30,
            1e30,
            5.0,
            1.0e5,
            440.0,
        ];
        for &manual in &hostile {
            for &off in &hostile {
                let mut r = RootFollow::new(RATE);
                r.set_params(&RootFollowParams {
                    tracking: true,
                    manual_root_hz: manual,
                    offset_semitones: off,
                    offset_cents: off,
                });
                for &e in &hostile {
                    for &c in &[0.0, 0.9, 1.0, f32::NAN, f32::INFINITY] {
                        let out = r.process_block(e, c);
                        assert!(
                            out.is_finite() && out >= spec.min && out <= spec.max,
                            "{out}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn switching_tracking_on_restarts_from_the_manual_root() {
        let mut r = on(110.0);
        for _ in 0..1000 {
            r.process_block(440.0, 0.9);
        }
        r.set_params(&RootFollowParams {
            tracking: false,
            manual_root_hz: 110.0,
            ..RootFollowParams::default()
        });
        assert_eq!(r.process_block(440.0, 0.9).to_bits(), 110.0f32.to_bits());
        r.set_params(&RootFollowParams {
            tracking: true,
            manual_root_hz: 110.0,
            ..RootFollowParams::default()
        });
        let out = r.process_block(0.0, 0.0);
        assert!((out - 110.0).abs() < 1e-3, "{out}");
    }

    #[cfg(feature = "pitch")]
    #[test]
    fn threshold_equals_the_trackers_default() {
        assert_eq!(CONFIDENCE_THRESHOLD, crate::pitch::PITCH_PARAMS[0].default);
    }
}
