use clap::{Parser, Subcommand};

mod generate;

#[derive(Parser)]
#[command(
    name = "asperitas-cli",
    about = "Offline WAV processing for asperitas DSP"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Process an input WAV through a DSP processor.
    Process(ProcessArgs),
    /// Generate a synthetic test signal.
    Generate(generate::GenerateArgs),
    /// Render a manifest of cases into one self-contained HTML review page.
    Review(ReviewArgs),
    /// Write the resonator's impulse and frequency response as CSV and a PNG plot.
    Analyze(AnalyzeArgs),
    /// Run the pitch tracker over a WAV or a built-in signal and write estimate against time as CSV and a PNG.
    Pitch(PitchArgs),
    /// Run the pitch tracker over the recorded corpus and the built-in signals and write a table.
    PitchCorpus(PitchCorpusArgs),
}

#[derive(Parser)]
struct PitchArgs {
    /// Input WAV file, or synthetic:sweep, synthetic:notes or synthetic:noise.
    input: String,
    /// Output prefix: writes <prefix>.csv and <prefix>.png.
    output_prefix: String,
    /// The signal is a chord: score confidence only.
    #[clap(long)]
    chordal: bool,
    /// The pitch the clip is meant to hold; reports the reference's offset from it.
    #[clap(long)]
    nominal_hz: Option<f64>,
    /// Tracker clarity threshold, 0.5 to 0.99.
    #[clap(long, default_value_t = 0.8)]
    clarity_threshold: f32,
    /// Width of "correct" in cents.
    #[clap(long, default_value_t = 50.0)]
    tolerance_cents: f64,
}

#[derive(Parser)]
struct PitchCorpusArgs {
    /// Directory to write <clip>.csv, <clip>.png and table.md into.
    output_dir: String,
    /// Directory of corpus WAV files.
    #[clap(long, default_value = "audio/instruments")]
    corpus_dir: String,
    /// Tracker clarity threshold, 0.5 to 0.99.
    #[clap(long, default_value_t = 0.8)]
    clarity_threshold: f32,
    /// Width of "correct" in cents.
    #[clap(long, default_value_t = 50.0)]
    tolerance_cents: f64,
}

#[derive(Parser)]
struct AnalyzeArgs {
    /// Output prefix: writes <prefix>_impulse.csv, <prefix>_response.csv and <prefix>.png.
    output_prefix: String,
    /// Resonator settings as key=value pairs, as `process --params`.
    #[clap(short = 'p', long, num_args = 1..)]
    params: Vec<String>,
    /// Sample rate in Hz.
    #[clap(long, default_value_t = 48_000)]
    sample_rate: u32,
    /// Impulse length in seconds (default: the root T60, at most 10).
    #[clap(long)]
    length_s: Option<f32>,
}

#[derive(Parser)]
struct ReviewArgs {
    /// Manifest file (TOML); see docs/reference/review.md.
    manifest: String,
    /// HTML page to write.
    output: String,
}

#[derive(Parser)]
struct ProcessArgs {
    /// Input WAV file.
    input: String,
    /// Output WAV file.
    output: String,
    /// Processor name (gain, filter, asperitas, resonator).
    #[clap(long)]
    processor: String,
    /// Parameters as key=value pairs.
    #[clap(short = 'p', long, num_args = 1..)]
    params: Vec<String>,
    /// Knob automation file (asperitas and resonator processors); see docs/reference/automation.md.
    #[clap(long)]
    automation: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Process(args) => {
            let proc_args = asperitas_cli::process::ProcessArgs {
                input_path: args.input,
                output_path: args.output,
                processor_name: args.processor,
                params: args.params,
                automation: args.automation,
            };
            asperitas_cli::process::run_process(&proc_args)
        }
        Command::Analyze(args) => {
            asperitas_cli::analyze::run_analyze(&asperitas_cli::analyze::AnalyzeArgs {
                params: args.params,
                sample_rate: args.sample_rate,
                length_s: args.length_s,
                output_prefix: args.output_prefix,
            })
            .map(|report| print!("{report}"))
        }
        Command::Pitch(args) => {
            asperitas_cli::pitch_report::run_pitch(&asperitas_cli::pitch_report::PitchArgs {
                input: args.input,
                output_prefix: args.output_prefix,
                chordal: args.chordal,
                nominal_hz: args.nominal_hz,
                clarity_threshold: args.clarity_threshold,
                tolerance_cents: args.tolerance_cents,
            })
            .map(|table| print!("{table}"))
        }
        Command::PitchCorpus(args) => {
            asperitas_cli::pitch_report::run_corpus(&asperitas_cli::pitch_report::CorpusArgs {
                corpus_dir: args.corpus_dir,
                output_dir: args.output_dir,
                clarity_threshold: args.clarity_threshold,
                tolerance_cents: args.tolerance_cents,
            })
            .map(|table| print!("{table}"))
        }
        Command::Generate(args) => generate::run_generate(&args),
        Command::Review(args) => {
            asperitas_cli::review::run_review(&asperitas_cli::review::ReviewArgs {
                manifest_path: args.manifest,
                output_path: args.output,
            })
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
