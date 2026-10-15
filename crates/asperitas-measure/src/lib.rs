//! Host-only analysis of a rig loopback measurement: the captured PCM, the stimulus that was
//! played, and the limits a run is judged against.
//!
//! - [`capture`] reassembles a committed `.console.zst` into samples, strictly.
//! - [`stimulus`] regenerates the played signal from the same `asperitas-dsp` processors the rig
//!   firmware runs.
//! - [`align`] finds the loopback delay per window and flags slips.
//! - [`metrics`] reports level, silence, discontinuity candidates and spurs as JSON.
//! - [`param_step`] flags audible level steps from unsmoothed parameter changes.
//! - [`thresholds`] is the only place a numeric limit is written down.

pub mod align;
pub mod capture;
pub mod metrics;
pub mod param_step;
pub mod stimulus;
pub mod thresholds;
