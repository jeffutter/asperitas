#![no_std]

pub mod processor;
pub mod gain;
pub mod filter;
mod smooth;

pub use processor::{Frame, Processor};
pub use gain::{Gain, GainParams};
pub use filter::{OnePoleLowPass, FilterParams};
