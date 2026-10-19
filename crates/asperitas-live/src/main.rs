//! `asperitas-live`: run the Asperitas effect in real time on the default audio devices.

mod backend;
mod terminal;

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

    /// Stop after this many seconds. Without it the stream runs until esc (knob view) or Enter
    /// (when stdout is not a terminal) is pressed; if stdin is closed the stream runs until killed.
    #[arg(long, value_name = "SECONDS", value_parser = parse_seconds)]
    seconds: Option<f64>,
}

/// Longest accepted `--seconds`: a day, far below anything `Duration` or `Instant` overflow on.
const MAX_SECONDS: f64 = 86_400.0;

fn parse_seconds(text: &str) -> Result<f64, String> {
    let s: f64 = text.parse().map_err(|e| format!("not a number: {e}"))?;
    if s.is_finite() && s > 0.0 && s <= MAX_SECONDS {
        Ok(s)
    } else {
        Err(format!("must be greater than 0 and at most {MAX_SECONDS}"))
    }
}

fn run(args: &Args) -> Result<(), String> {
    let controls = parse_asperitas_controls(&args.params)?;
    let clip = match &args.input {
        Some(path) => Some(load_clip(&path.to_string_lossy())?),
        None => None,
    };
    let source = match &args.input {
        Some(path) => format!("loop {}", path.display()),
        None => "line-in".to_string(),
    };
    let seconds = args.seconds;
    backend::run(clip, &controls, args.buffer, |slot| {
        terminal::wait(slot, &controls, &source, seconds)
    })
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

#[cfg(test)]
mod tests {
    use super::parse_seconds;

    #[test]
    fn rejects_unusable_seconds() {
        for bad in ["inf", "NaN", "-1", "0", "1e300", "abc"] {
            assert!(parse_seconds(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn accepts_sane_seconds() {
        assert_eq!(parse_seconds("0.5"), Ok(0.5));
        assert_eq!(parse_seconds("3600"), Ok(3600.0));
    }
}
