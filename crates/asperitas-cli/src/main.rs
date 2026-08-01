use clap::{Parser, Subcommand};

mod generate;
mod process;
mod synth;
mod wav_io;

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
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Process(args) => {
            let proc_args = process::ProcessArgs {
                input_path: args.input,
                output_path: args.output,
                processor_name: args.processor,
                params: args.params,
            };
            process::run_process(&proc_args)
        }
        Command::Generate(args) => generate::run_generate(&args),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
