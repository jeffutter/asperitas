#![cfg_attr(not(feature = "std"), no_std)]

pub mod filter;
pub mod gain;
pub mod processor;
mod smooth;
pub mod stimulus;

pub use filter::{FilterParams, OnePoleLowPass};
pub use gain::{Gain, GainParams};
pub use processor::{Frame, Processor};
pub use stimulus::{
    ExponentialSweep, ExponentialSweepParams, PulseTrain, PulseTrainParams, Sine, SineParams,
    Stimulus,
};
