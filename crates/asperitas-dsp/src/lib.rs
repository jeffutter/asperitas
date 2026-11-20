#![cfg_attr(not(feature = "std"), no_std)]

pub mod asperitas;
#[cfg(feature = "std")]
pub mod automation;
pub mod excitation;
pub mod filter;
pub mod gain;
pub mod param;
pub mod pcm;
pub mod processor;
pub mod resonator;
mod smooth;

pub use asperitas::{Asperitas, Controls, ASPERITAS_PARAMS, KNOB_COUNT};
pub use excitation::{Excitation, ExcitationParams, EXCITATION_PARAMS, EXCITATION_PRESETS};
pub use filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
pub use gain::{Gain, GainParams, GAIN_PARAMS};
pub use param::{ParamSpec, Taper};
pub use processor::{Frame, Processor};
pub use resonator::{ResonatorVoice, ResonatorVoiceParams, RESONATOR_VOICE_PARAMS};
