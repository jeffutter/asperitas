#![cfg_attr(not(feature = "std"), no_std)]

pub mod asperitas;
#[cfg(feature = "std")]
pub mod automation;
pub mod envelope;
pub mod excitation;
pub mod filter;
pub mod gain;
pub mod param;
pub mod pcm;
#[cfg(feature = "pitch")]
pub mod pitch;
pub mod processor;
pub mod random_walk;
pub mod resonator;
pub mod resonator_bank;
pub mod resonator_controls;
mod smooth;

pub use asperitas::{Asperitas, Controls, ASPERITAS_PARAMS, KNOB_COUNT};
pub use envelope::{EnvelopeFollower, EnvelopeParams, ENVELOPE_PARAMS};
pub use excitation::{Excitation, ExcitationParams, EXCITATION_PARAMS, EXCITATION_PRESETS};
pub use filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
pub use gain::{Gain, GainParams, GAIN_PARAMS};
pub use param::{ParamSpec, Taper};
#[cfg(feature = "pitch")]
pub use pitch::{PitchParams, PitchTracker, PITCH_HOP, PITCH_PARAMS};
pub use processor::{Frame, Processor};
pub use random_walk::{RandomWalk, RandomWalkParams, RANDOM_WALK_PARAMS};
pub use resonator::{ResonatorVoice, ResonatorVoiceParams, RESONATOR_VOICE_PARAMS};
pub use resonator_bank::{Resonator, ResonatorParams, BANK_GAIN, RESONATOR_PARAMS, VOICE_COUNT};
pub use resonator_controls::{
    ResonatorControls, ResonatorControlsError, RESONATOR_CONTROL_PARAMS, RESONATOR_KNOB_COUNT,
};
