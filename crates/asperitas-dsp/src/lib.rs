#![cfg_attr(not(feature = "std"), no_std)]

pub mod asperitas;
pub mod filter;
pub mod gain;
pub mod param;
pub mod pcm;
pub mod processor;
mod smooth;

pub use asperitas::{Asperitas, Controls, ASPERITAS_PARAMS, KNOB_COUNT};
pub use filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
pub use gain::{Gain, GainParams, GAIN_PARAMS};
pub use param::{ParamSpec, Taper};
pub use processor::{Frame, Processor};
