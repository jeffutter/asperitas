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
}

#[derive(Parser)]
struct ProcessArgs {
    /// Input WAV file.
    input: String,
    /// Output WAV file.
    output: String,
    /// Processor name (gain, filter).
    #[clap(long)]
    processor: String,
    /// Parameters as key=value pairs.
    #[clap(short = 'p', long, num_args = 1..)]
    params: Vec<String>,
    /// Knob automation file (asperitas processor only); see docs/reference/automation.md.
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
        Command::Generate(args) => generate::run_generate(&args),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
