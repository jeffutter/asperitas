//! `asperitas-live`: run the Asperitas effect in real time on the default audio devices.

mod backend;
mod recorder;
mod terminal;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use asperitas_cli::process::{parse_asperitas_controls, parse_resonator_controls};
use asperitas_live::view::ViewSurface;
use asperitas_live::{load_clip, ControlSlot, EffectKind};
use clap::{ArgGroup, Parser, ValueEnum};

/// The effects `--effect` selects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum EffectArg {
    /// The low-pass and gain patch (`cutoff_hz`, `gain_db`).
    #[default]
    Asperitas,
    /// The resonator bank (`root_hz`, `structure`, `damping`, `spread`, `excitation_preset`).
    Resonator,
}

impl From<EffectArg> for EffectKind {
    fn from(arg: EffectArg) -> Self {
        match arg {
            EffectArg::Asperitas => EffectKind::Asperitas,
            EffectArg::Resonator => EffectKind::Resonator,
        }
    }
}

/// Play a looping clip or live line-in through an effect (Asperitas by default).
#[derive(Parser)]
#[command(version, group(ArgGroup::new("source").required(true).args(["input", "line_in"])))]
struct Args {
    /// 48 kHz WAV clip to play in a loop.
    #[arg(long, value_name = "WAV")]
    input: Option<PathBuf>,

    /// Use the default input device instead of a clip.
    #[arg(long)]
    line_in: bool,

    /// Effect to run.
    #[arg(long, value_enum, default_value_t = EffectArg::Asperitas)]
    effect: EffectArg,

    /// Knob values in real units, as for `asperitas-cli process` with the same effect:
    /// `cutoff_hz=2000 gain_db=-6` for asperitas, `root_hz=220 damping=0.4` for resonator.
    #[arg(long, num_args = 1.., value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Host buffer size in frames; 0 takes the device default.
    #[arg(long, default_value_t = 128)]
    buffer: u32,

    /// Also write what is sent to the output device (after the clamp) to this 48 kHz stereo
    /// 32-bit float WAV, overwriting it.
    #[arg(long, value_name = "WAV")]
    record: Option<PathBuf>,

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
    match args.effect {
        EffectArg::Asperitas => run_with(args, parse_asperitas_controls(&args.params)?),
        EffectArg::Resonator => run_with(args, parse_resonator_controls(&args.params)?),
    }
}

fn run_with<C: ViewSurface>(args: &Args, controls: C) -> Result<(), String> {
    let clip = match &args.input {
        Some(path) => Some(load_clip(&path.to_string_lossy())?),
        None => None,
    };
    let source = match &args.input {
        Some(path) => format!("loop {}", path.display()),
        None => "line-in".to_string(),
    };
    let seconds = args.seconds;
    backend::run(
        clip,
        args.effect.into(),
        Arc::new(ControlSlot::new(&controls)),
        args.buffer,
        args.record.as_deref(),
        |slot, error| terminal::wait(slot, error, &controls, &source, seconds),
    )
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
    use super::{parse_seconds, Args, EffectArg};
    use clap::Parser;

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

    #[test]
    fn no_selector_runs_the_asperitas_effect() {
        let args = Args::parse_from(["asperitas-live", "--line-in"]);
        assert_eq!(args.effect, EffectArg::Asperitas);
        let args = Args::parse_from(["asperitas-live", "--line-in", "--effect", "resonator"]);
        assert_eq!(args.effect, EffectArg::Resonator);
        assert!(Args::try_parse_from(["asperitas-live", "--line-in", "--effect", "x"]).is_err());
    }

    #[test]
    fn params_are_parsed_for_the_selected_effect() {
        use asperitas_cli::process::{parse_asperitas_controls, parse_resonator_controls};
        let root = ["root_hz=220".to_string()];
        assert!(parse_resonator_controls(&root).is_ok());
        assert!(parse_asperitas_controls(&root).is_err());
        let cutoff = ["cutoff_hz=2000".to_string()];
        assert!(parse_asperitas_controls(&cutoff).is_ok());
        let err = parse_resonator_controls(&cutoff).unwrap_err();
        assert!(
            err.contains("cutoff_hz") && err.contains("resonator"),
            "{err}"
        );
    }
}
