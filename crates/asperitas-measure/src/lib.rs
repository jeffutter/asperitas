//! Host-only analysis of a rig loopback measurement: the captured PCM, the stimulus that was
//! played, and the limits a run is judged against.
//!
//! - [`capture`] reassembles a committed `.console.zst` into samples, strictly.
//! - [`stimulus`] regenerates the played signal from the same `asperitas-dsp` processors the rig
//!   firmware runs.
//! - [`thresholds`] is the only place a numeric limit is written down.

pub mod capture;
pub mod stimulus;
pub mod thresholds;
