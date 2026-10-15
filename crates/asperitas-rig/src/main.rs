//! `asperitas-rig`: capture the device's framed USB console for a fixed time, write an
//! integrity-checked artifact, and exit non-zero when the capture is not trustworthy; or
//! send control commands (query, restart, knob override) over the same link.
//!
//! Exit codes: 0 pass, 1 failed verdict or device-rejected command, 2 usage/device/I-O
//! error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use asperitas_rig::cli::{list_candidates, pick_device, run, Deadline, Outcome};
use asperitas_rig::control::{
    query, restart, run_override, schedule, ControlError, Link, OverridePlan, RestartOpts,
};
use asperitas_rig::Policy;
use clap::{Args, Parser, Subcommand};
use serialport::SerialPort;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Capture the console for a fixed time and judge the capture.
    Capture {
        #[command(flatten)]
        device: DeviceArg,
        /// Capture length in seconds.
        #[arg(long)]
        duration: f64,
        /// Artifact output path.
        #[arg(long)]
        out: PathBuf,
        /// Device-reported drops tolerated before the capture fails.
        #[arg(long, default_value_t = 0)]
        max_drops: u64,
    },
    /// Print the firmware version and last reset reason.
    Query {
        #[command(flatten)]
        device: DeviceArg,
    },
    /// Restart the device, wait for the console to re-enumerate, and query the new boot.
    Restart {
        #[command(flatten)]
        device: DeviceArg,
        /// Seconds to wait for the console to come back.
        #[arg(long, default_value_t = 15.0)]
        reenumerate_timeout: f64,
    },
    /// Hold or sweep a knob override while capturing, then release it.
    Override {
        #[command(flatten)]
        device: DeviceArg,
        /// Knob id (decimal, as in the firmware knob table).
        #[arg(long)]
        knob: u8,
        /// Hold the knob at this level, 0..=1, for the whole duration.
        #[arg(long, conflicts_with = "sweep", required_unless_present = "sweep")]
        value: Option<f64>,
        /// Sweep the knob linearly `FROM:TO` (levels 0..=1) over the duration.
        #[arg(long)]
        sweep: Option<String>,
        /// Capture and override length in seconds.
        #[arg(long)]
        duration: f64,
        /// Artifact output path.
        #[arg(long)]
        out: PathBuf,
        /// Device-reported drops tolerated before the capture fails.
        #[arg(long, default_value_t = 0)]
        max_drops: u64,
    },
}

#[derive(Args)]
struct DeviceArg {
    /// Serial device; auto-detected (/dev/cu.usbmodem*, /dev/ttyACM*) when omitted.
    #[arg(long)]
    device: Option<PathBuf>,
}

fn main() -> ExitCode {
    match go(Cli::parse().command) {
        Ok(o) => ExitCode::from(o.code()),
        Err(e) => {
            eprintln!("asperitas-rig: {e}");
            ExitCode::from(e.outcome().code())
        }
    }
}

fn seconds(name: &str, s: f64) -> Result<Duration, ControlError> {
    Duration::try_from_secs_f64(s).map_err(|e| ControlError::Usage(format!("invalid {name}: {e}")))
}

fn candidates(explicit: &Option<PathBuf>) -> Result<Vec<PathBuf>, ControlError> {
    Ok(match explicit {
        Some(p) => vec![p.clone()],
        None => list_candidates(Path::new("/dev"))?,
    })
}

fn open(path: &Path) -> Result<Box<dyn SerialPort>, ControlError> {
    // CDC ignores baud/framing; the short timeout lets deadlines fire on an idle port.
    serialport::new(path.to_string_lossy(), 115_200)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| ControlError::NoDevice(format!("cannot open {}: {e}", path.display())))
}

fn pick(device: &DeviceArg) -> Result<PathBuf, ControlError> {
    let list = if device.device.is_some() {
        Vec::new()
    } else {
        candidates(&None)?
    };
    pick_device(device.device.clone(), list).map_err(|e| ControlError::NoDevice(e.to_string()))
}

fn parse_sweep(s: &str) -> Result<(f64, f64), ControlError> {
    let bad = || ControlError::Usage(format!("--sweep wants FROM:TO levels, got {s:?}"));
    let (a, b) = s.split_once(':').ok_or_else(bad)?;
    Ok((a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?))
}

fn go(command: Command) -> Result<Outcome, ControlError> {
    const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
    match command {
        Command::Capture {
            device,
            duration,
            out,
            max_drops,
        } => {
            let duration = seconds("--duration", duration)?;
            let port = open(&pick(&device)?)?;
            let policy = Policy {
                max_device_dropped: max_drops,
                require_status: true,
            };
            let reader = Deadline::new(port, Instant::now() + duration);
            Ok(run(reader, &policy, &out, &mut std::io::stdout())?)
        }
        Command::Query { device } => {
            let mut link = Link::new(open(&pick(&device)?)?, REPLY_TIMEOUT);
            println!("{}", query(&mut link)?);
            if link.bad_frames() > 0 {
                eprintln!(
                    "asperitas-rig: warning: {} bad frame(s) while waiting for replies",
                    link.bad_frames()
                );
            }
            Ok(Outcome::Pass)
        }
        Command::Restart {
            device,
            reenumerate_timeout,
        } => {
            let link = Link::new(open(&pick(&device)?)?, REPLY_TIMEOUT);
            let explicit = device.device.clone();
            let r = restart(
                link,
                || {
                    let all =
                        candidates(&explicit).map_err(|e| std::io::Error::other(e.to_string()))?;
                    Ok(all.into_iter().filter(|p| p.exists()).collect())
                },
                |p| open(p).map_err(|e| std::io::Error::other(e.to_string())),
                std::thread::sleep,
                RestartOpts {
                    reenumerate_timeout: seconds("--reenumerate-timeout", reenumerate_timeout)?,
                    poll: Duration::from_millis(100),
                },
            )?;
            println!(
                "restarted device={} {} boot_record={}",
                r.path.display(),
                r.identity,
                r.boot_record
            );
            Ok(Outcome::Pass)
        }
        Command::Override {
            device,
            knob,
            value,
            sweep,
            duration,
            out,
            max_drops,
        } => {
            let (from, to) = match (value, sweep) {
                (Some(v), None) => (v, v),
                (None, Some(s)) => parse_sweep(&s)?,
                _ => unreachable!("clap enforces exactly one of --value and --sweep"),
            };
            let plan = OverridePlan {
                knob,
                from,
                to,
                duration: seconds("--duration", duration)?,
            };
            // Reject bad levels before touching the port.
            schedule(plan.from, plan.to, plan.duration)?;
            let port = open(&pick(&device)?)?;
            let writer = port
                .try_clone()
                .map_err(|e| ControlError::NoDevice(format!("cannot clone port: {e}")))?;
            let policy = Policy {
                max_device_dropped: max_drops,
                require_status: true,
            };
            run_override(port, writer, &plan, &policy, &out, &mut std::io::stdout())
        }
    }
}
