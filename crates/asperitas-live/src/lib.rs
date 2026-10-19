//! Real-time host runner for the Asperitas effect.
//!
//! The processing core ([`Engine`], [`ControlSlot`], [`LoopSource`]) has no audio-device types
//! and runs from a test at any buffer length; the `asperitas-live` binary wraps it in `cpal`
//! streams.

pub mod controls;
pub mod engine;

pub use controls::ControlSlot;
pub use engine::{load_clip, Engine, LoopSource, Source, BLOCK_FRAMES, SAMPLE_RATE};
