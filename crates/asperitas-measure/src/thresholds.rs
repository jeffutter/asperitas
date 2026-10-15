//! Every numeric limit the measurement analysis judges against.
//!
//! Nothing outside this module may spell a limit as a literal. Each constant cites where its value
//! came from, so a change to one is a change to a documented decision.

/// Loop gain of the Pod self-loopback at the rig's -20 dBFS sine, in dB (capture minus stimulus).
///
/// Origin: TASK-038.05 baseline, 2026-10-07 300 s run (`audio/captures/2026-10-07-rig-sine-300s.console.zst`).
pub const LOOP_GAIN_DB: f64 = -0.41;

/// Noise plus distortion of that capture, in LSB RMS of the 16-bit capture.
///
/// Origin: TASK-038.05 baseline, same run.
pub const NOISE_DISTORTION_LSB_RMS: f64 = 0.35;

/// DC offset of that capture, in 16-bit LSB.
///
/// Origin: TASK-038.05 baseline, same run.
pub const DC_LSB: f64 = -0.49;

/// Alignment window, samples (1 s at 48 kHz). Long enough for a sharp correlation peak against
/// the rig's noise floor, short enough that a slip is localised to a second.
///
/// Origin: chosen for TASK-035.02; confirmed against the 2026-10-07 and 2026-10-13 fixtures.
pub const ALIGN_WINDOW_SAMPLES: usize = 48_000;

/// Largest loopback delay searched, samples. Delay is determined modulo the period for a periodic
/// stimulus, so this is also the span over which a sine delay is unambiguous only if below one period.
///
/// Origin: TASK-035.02, set from measured fixture delays (see tests in `align.rs`).
pub const ALIGN_MAX_DELAY_SAMPLES: usize = 1_000;

/// Largest allowed change in delay between windows or repeat captures, samples.
///
/// Origin: TASK-035.02, set from measured fixture spread (see tests in `align.rs`).
pub const ALIGN_STABLE_TOL_SAMPLES: f64 = 0.25;

/// Half-width of the delay search around the first window's delay once anchored, samples. Below
/// half the shortest stimulus period (48 samples for the rig's 1 kHz sine) so tracking cannot jump
/// to a neighbouring period, and above any plausible one-window slip.
///
/// Origin: TASK-035.02.
pub const ALIGN_TRACK_SAMPLES: usize = 8;

/// A correlation peak counts as tied with the maximum above this fraction of it. Used only to pick
/// the smallest delay among the equal peaks a periodic stimulus produces, so the anchor is the same
/// across repeat captures.
///
/// Origin: TASK-035.02; fixture sine peaks agree to far better than this.
pub const ALIGN_TIE_FRACTION: f32 = 0.98;
