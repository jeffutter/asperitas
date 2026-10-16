#![cfg_attr(not(feature = "std"), no_std)]

pub mod filter;
pub mod fx_chain;
pub mod gain;
pub mod processor;
pub mod replay_fx;
mod smooth;
pub mod stimulus;

pub use filter::{FilterParams, OnePoleLowPass};
pub use fx_chain::FxChain;
pub use gain::{Gain, GainParams};
pub use processor::{Frame, Processor};
pub use replay_fx::{FxKind, ReplayFx};
pub use stimulus::{
    ExponentialSweep, ExponentialSweepParams, PulseTrain, PulseTrainParams, Sine, SineParams,
    Stimulus,
};
