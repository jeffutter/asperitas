//! `asperitas-rig`: capture the device's framed USB console for a fixed time, write an
//! integrity-checked artifact, and exit non-zero when the capture is not trustworthy; or
//! send control commands (query, restart, knob override) over the same link, or run a whole
//! bench session and keep the raw stream.
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
use asperitas_rig::session::{
    probe_reset, run_session, write_session, Motion, SessionOpts, SessionPlan, DEFAULT_SNAPS,
    DEFAULT_SNAP_LEVEL,
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
    /// Run one bench session: select the effect on a fresh boot, drive a knob once the capture
    /// arms, release it, and keep the raw stream (zstd) plus a `<out>.session` sidecar.
    Session {
        #[command(flatten)]
        device: DeviceArg,
        /// Raw stream output path (zstd); the sidecar is written beside it as `<out>.session`.
        #[arg(long)]
        out: PathBuf,
        /// Effect selection: 0 bypass, 1 patch.
        #[arg(long)]
        fx_f: u8,
        /// Value sent with the effect selection (the rig ignores it).
        #[arg(long, default_value_t = 0)]
        fx_v: u16,
        /// Knob id (decimal): 0 filter cutoff, 1 output gain. Required: sweep and hold levels mean
        /// different things on each knob, so there is no safe default.
        #[arg(long)]
        knob: u8,
        /// Give up this many seconds after attaching.
        #[arg(long)]
        max_seconds: f64,
        /// Reset the board over the probe first and re-attach when the console returns.
        #[arg(long)]
        probe_reset: bool,
        /// Chip description for `--probe-reset`.
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../firmware/asperitas-h750.yaml"))]
        chip_description: PathBuf,
        #[command(subcommand)]
        motion: MotionArg,
    },
}

/// How the session drives the knob.
#[derive(Subcommand)]
enum MotionArg {
    /// Linear sweep at 20 steps per second.
    Sweep {
        /// Start level, 0..=1.
        #[arg(long)]
        from: f64,
        /// End level, 0..=1.
        #[arg(long)]
        to: f64,
        /// Sweep length in seconds.
        #[arg(long)]
        duration: f64,
        /// Seconds between the capture arming and the first step.
        #[arg(long, default_value_t = 0.0)]
        start_after_arm: f64,
    },
    /// 250 s triangle ramp from 0.50 to 0.95 starting 10 s after arming, with two snaps.
    Soak {
        /// Leave out the snaps.
        #[arg(long)]
        no_snaps: bool,
        /// Level added during each 0.5 s snap.
        #[arg(long, default_value_t = DEFAULT_SNAP_LEVEL)]
        snap_level: f64,
    },
    /// One level, set before the capture arms and held until the dump ends.
    Hold {
        /// Level, 0..=1.
        #[arg(long)]
        level: f64,
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
    open_with_timeout(path, Duration::from_millis(100))
}

fn open_with_timeout(path: &Path, timeout: Duration) -> Result<Box<dyn SerialPort>, ControlError> {
    // CDC ignores baud/framing; the short timeout lets deadlines fire on an idle port.
    serialport::new(path.to_string_lossy(), 115_200)
        .timeout(timeout)
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

/// Open the console for a session. After a probe reset the node vanishes and comes back, possibly
/// under another number, so keep looking until it opens or `wait` runs out.
fn attach(
    device: &DeviceArg,
    wait: Duration,
    timeout: Duration,
) -> Result<Box<dyn SerialPort>, ControlError> {
    let give_up = Instant::now() + wait;
    loop {
        let attempt = pick(device).and_then(|p| open_with_timeout(&p, timeout));
        match attempt {
            Ok(port) => return Ok(port),
            Err(e) if Instant::now() >= give_up => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn session_motion(m: MotionArg) -> Result<Motion, ControlError> {
    Ok(match m {
        MotionArg::Sweep {
            from,
            to,
            duration,
            start_after_arm,
        } => Motion::Sweep {
            from,
            to,
            duration: seconds("--duration", duration)?,
            start_after_arm: seconds("--start-after-arm", start_after_arm)?,
        },
        MotionArg::Soak {
            no_snaps,
            snap_level,
        } => Motion::Soak {
            snaps: if no_snaps {
                Vec::new()
            } else {
                DEFAULT_SNAPS.to_vec()
            },
            snap_level,
        },
        MotionArg::Hold { level } => Motion::Hold { level },
    })
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
            };
            run_override(port, writer, &plan, &policy, &out, &mut std::io::stdout())
        }
        Command::Session {
            device,
            out,
            fx_f,
            fx_v,
            knob,
            max_seconds,
            probe_reset: reset,
            chip_description,
            motion,
        } => {
            let plan = SessionPlan {
                fx_f,
                fx_v,
                knob,
                motion: session_motion(motion)?,
            };
            // Reject bad levels and lengths before touching the board.
            plan.timeline()?;
            let opts = SessionOpts::bench(seconds("--max-seconds", max_seconds)?);
            // The schedule runs between reads, so the read timeout is its jitter.
            let poll = Duration::from_millis(10);
            let port = if reset {
                let before = pick(&device).ok();
                probe_reset(&chip_description)?;
                println!("probe reset done");
                // Wait for the old node to go (a fast reboot may beat the poll) before attaching.
                if let Some(old) = before {
                    let gone = Instant::now() + Duration::from_secs(6);
                    while old.exists() && Instant::now() < gone {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                attach(&device, Duration::from_secs(15), poll)?
            } else {
                attach(&device, Duration::ZERO, poll)?
            };
            let run = run_session(port, &plan, &opts);
            write_session(&out, &plan, &run)?;
            println!("{}", run.summary());
            if let Some(e) = run.error {
                return Err(e);
            }
            let outcome = run.outcome();
            if outcome != Outcome::Pass {
                println!("session acks incomplete or device rejected a command");
            }
            Ok(outcome)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    const BASE: [&str; 9] = [
        "asperitas-rig",
        "session",
        "--out",
        "run.zst",
        "--fx-f",
        "1",
        "--max-seconds",
        "30",
        "soak",
    ];

    #[test]
    fn session_requires_an_explicit_knob() {
        assert!(Cli::try_parse_from(BASE).is_err());
        let mut with_knob = BASE.to_vec();
        with_knob.splice(2..2, ["--knob", "1"]);
        assert!(Cli::try_parse_from(with_knob).is_ok());
    }
}
