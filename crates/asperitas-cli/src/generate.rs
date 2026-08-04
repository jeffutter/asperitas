//! Generate synthetic test signals and write them as WAV files.

use clap::Parser;

use crate::synth;
use crate::wav_io;

/// Signal type to generate.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SignalType {
    Impulse,
    Sweep,
    Pluck,
}

/// Arguments for the generate subcommand.
#[derive(Parser, Debug)]
pub struct GenerateArgs {
    /// Output WAV file path.
    #[clap(short = 'o', long)]
    pub output: String,

    /// Type of signal to generate.
    #[clap(short = 't', long, value_enum, default_value = "impulse")]
    pub signal: SignalType,

    /// Duration in seconds.
    #[clap(short = 'd', long, default_value = "1.0")]
    pub duration: f32,

    /// Sample rate in Hz.
    #[clap(short = 'r', long, default_value = "48000")]
    pub sample_rate: u32,
}

/// Run the generate command: create a synthetic signal and write it as WAV.
pub fn run_generate(args: &GenerateArgs) -> Result<(), String> {
    let mono = match args.signal {
        SignalType::Impulse => synth::generate_impulse(args.sample_rate, args.duration),
        SignalType::Sweep => synth::generate_sweep(args.sample_rate, args.duration),
        SignalType::Pluck => synth::generate_pluck(args.sample_rate, args.duration, 440.0),
    };

    let frames = synth::to_stereo(&mono);

    wav_io::write_wav(&args.output, args.sample_rate, &frames)?;
    Ok(())
}
