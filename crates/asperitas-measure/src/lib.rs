//! Host-only analysis of a rig loopback measurement: the captured PCM, the stimulus that was
//! played, and the limits a run is judged against.
//!
//! - [`capture`] reassembles a committed `.console.zst` into samples, strictly.
//! - [`stimulus`] regenerates the played signal from the same `asperitas-dsp` processors the rig
//!   firmware runs.
//! - [`align`] finds the loopback delay per window and flags slips.
//! - [`baseline`] judges a steady-stimulus loopback capture against the rig baseline.
//! - [`compare`] checks a replayed excerpt against the `asperitas-cli` reference.
//! - [`metrics`] reports level, silence, discontinuity candidates and spurs as JSON.
//! - [`param_step`] flags audible level steps from unsmoothed parameter changes.
//! - [`sweep`] judges a knob sweep: response direction, monotonicity and flat regions.
//! - [`soak`] judges a long capture while a parameter moves: dropouts, discontinuities and steps.
//! - [`cli`] is the `asperitas-measure analyze` command over all of the above.
//! - [`thresholds`] is the only place a numeric limit is written down.

pub mod align;
pub mod baseline;
pub mod capture;
pub mod cli;
pub mod compare;
pub mod metrics;
pub mod param_step;
pub mod soak;
pub mod stimulus;
pub mod sweep;
pub mod thresholds;
