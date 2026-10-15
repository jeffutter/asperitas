//! `asperitas-measure`: see [`asperitas_measure::cli`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome =
        asperitas_measure::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(outcome.code())
}
