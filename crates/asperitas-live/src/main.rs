//! `asperitas-live`: run the Asperitas effect in real time on the default audio devices.

mod backend;

use std::path::PathBuf;
use std::process::ExitCode;

use asperitas_cli::process::parse_asperitas_controls;
use clap::{ArgGroup, Parser};

use asperitas_live::load_clip;

/// Play a looping clip or live line-in through the Asperitas effect.
#[derive(Parser)]
#[command(version, group(ArgGroup::new("source").required(true).args(["input", "line_in"])))]
struct Args {
    /// 48 kHz WAV clip to play in a loop.
    #[arg(long, value_name = "WAV")]
    input: Option<PathBuf>,

    /// Use the default input device instead of a clip.
    #[arg(long)]
    line_in: bool,

    /// Knob values in real units, as for `asperitas-cli process`: `cutoff_hz=2000 gain_db=-6`.
    #[arg(long, num_args = 1.., value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Host buffer size in frames; 0 takes the device default.
    #[arg(long, default_value_t = 128)]
    buffer: u32,

    /// Stop after this many seconds. Without it the stream runs until Enter is pressed.
    #[arg(long, value_name = "SECONDS")]
    seconds: Option<f64>,
}

fn run(args: &Args) -> Result<(), String> {
    let controls = parse_asperitas_controls(&args.params)?;
    let clip = match &args.input {
        Some(path) => Some(load_clip(&path.to_string_lossy())?),
        None => None,
    };
    backend::run(clip, &controls, args.buffer, args.seconds)
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("asperitas-live: {message}");
            ExitCode::FAILURE
        }
    }
}
