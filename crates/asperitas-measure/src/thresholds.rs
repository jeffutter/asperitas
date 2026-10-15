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
