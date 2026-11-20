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
