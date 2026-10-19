#![cfg_attr(not(feature = "std"), no_std)]

pub mod asperitas;
pub mod filter;
pub mod fx_chain;
pub mod gain;
pub mod param;
pub mod pcm;
pub mod processor;
pub mod replay_fx;
mod smooth;
pub mod stimulus;

pub use asperitas::{Asperitas, Controls, ASPERITAS_PARAMS, KNOB_COUNT};
pub use filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
pub use fx_chain::FxChain;
pub use gain::{Gain, GainParams, GAIN_PARAMS};
pub use param::{ParamSpec, Taper};
pub use processor::{Frame, Processor};
pub use replay_fx::{FxKind, ReplayFx};
pub use stimulus::{
    ExponentialSweep, ExponentialSweepParams, PulseTrain, PulseTrainParams, Sine, SineParams,
    Stimulus,
};
