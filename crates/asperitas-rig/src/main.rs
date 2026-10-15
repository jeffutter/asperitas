//! `asperitas-rig`: capture the device's framed USB console for a fixed time, write an
//! integrity-checked artifact, and exit non-zero when the capture is not trustworthy.
//!
//! Exit codes: 0 pass, 1 failed verdict, 2 usage/device/I-O error.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use asperitas_rig::cli::{list_candidates, pick_device, run, Deadline, Outcome};
use asperitas_rig::Policy;
use clap::Parser;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Serial device; auto-detected (/dev/cu.usbmodem*, /dev/ttyACM*) when omitted.
    #[arg(long)]
    device: Option<PathBuf>,
    /// Capture length in seconds.
    #[arg(long)]
    duration: f64,
    /// Artifact output path.
    #[arg(long)]
    out: PathBuf,
    /// Device-reported drops tolerated before the capture fails.
    #[arg(long, default_value_t = 0)]
    max_drops: u64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match go(args) {
        Ok(o) => ExitCode::from(o.code()),
        Err(e) => {
            eprintln!("asperitas-rig: {e}");
            ExitCode::from(Outcome::Error.code())
        }
    }
}

fn go(args: Args) -> Result<Outcome, Box<dyn std::error::Error>> {
    let duration = Duration::try_from_secs_f64(args.duration)
        .map_err(|e| format!("invalid --duration: {e}"))?;
    let candidates = if args.device.is_some() {
        Vec::new()
    } else {
        list_candidates(std::path::Path::new("/dev"))?
    };
    let path = pick_device(args.device, candidates)?;
    // CDC ignores baud/framing; the short timeout lets the deadline fire on an idle port.
    let port = serialport::new(path.to_string_lossy(), 115_200)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let policy = Policy {
        max_device_dropped: args.max_drops,
        require_status: true,
    };
    let reader = Deadline::new(port, Instant::now() + duration);
    Ok(run(reader, &policy, &args.out, &mut std::io::stdout())?)
}
