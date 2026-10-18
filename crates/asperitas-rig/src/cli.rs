//! Testable pieces of the `asperitas-rig` binary: device selection, the timed reader, and
//! the run-and-judge core. `main.rs` only parses arguments and opens the serial port.

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{capture, render, verdict, Capture, Policy};

pub use asperitas_host::outcome::Outcome;

/// Why no single device could be chosen.
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceError {
    /// Nothing matched the console port patterns.
    None,
    /// More than one port matched; the caller must pick with `--device`.
    Ambiguous(Vec<PathBuf>),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::None => write!(
                f,
                "no console device found (looked for /dev/cu.usbmodem* and /dev/ttyACM*); \
                 is the board plugged in? pass --device to override"
            ),
            DeviceError::Ambiguous(c) => {
                let list: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "multiple console devices found: {}; pass --device to choose",
                    list.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for DeviceError {}

/// True for file names of USB CDC serial ports: macOS callout (`cu.usbmodem*`, not the
/// blocking `tty.*` twin) and Linux `ttyACM*`.
pub fn is_console_name(name: &str) -> bool {
    name.starts_with("cu.usbmodem") || name.starts_with("ttyACM")
}

/// List candidate ports in `dir`, sorted.
pub fn list_candidates(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(is_console_name) {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// Choose the port: an explicit path wins; otherwise exactly one candidate is required.
pub fn pick_device(
    explicit: Option<PathBuf>,
    mut candidates: Vec<PathBuf>,
) -> Result<PathBuf, DeviceError> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    match candidates.len() {
        0 => Err(DeviceError::None),
        1 => Ok(candidates.remove(0)),
        _ => Err(DeviceError::Ambiguous(candidates)),
    }
}

/// Reader adapter that reports EOF once `deadline` passes, so [`capture`] runs unchanged.
///
/// Timeouts from the inner reader (a serial port with a short read timeout) are turned into
/// `Interrupted`, which `capture` retries; they never end the capture early. The deadline is
/// re-checked on every call, so an idle port still stops on time.
pub struct Deadline<R> {
    inner: R,
    deadline: Instant,
}

impl<R> Deadline<R> {
    /// Wrap `inner`, ending at `deadline`.
    pub fn new(inner: R, deadline: Instant) -> Self {
        Self { inner, deadline }
    }
}

impl<R: Read> Read for Deadline<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if Instant::now() >= self.deadline {
            return Ok(0);
        }
        match self.inner.read(buf) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            }
            other => other,
        }
    }
}

/// Write the artifact for `cap` to `out`, print its summary (the artifact up to its record
/// list) to `stdout`, and judge it. Returns the outcome and the rendered artifact.
pub fn report(
    cap: &Capture,
    policy: &Policy,
    out: &Path,
    stdout: &mut dyn io::Write,
) -> io::Result<Outcome> {
    let text = render(cap, policy);
    std::fs::write(out, &text)?;
    let summary = text
        .split_once("--\n")
        .map_or(text.as_str(), |(head, _)| head);
    stdout.write_all(summary.as_bytes())?;
    let outcome = if verdict(&cap.summary, policy).is_pass() {
        Outcome::Pass
    } else {
        Outcome::Fail
    };
    Ok(outcome)
}

/// Capture from `reader` to EOF, write the artifact to `out`, print the summary to
/// `stdout`, and judge it.
pub fn run<R: Read>(
    reader: R,
    policy: &Policy,
    out: &Path,
    stdout: &mut dyn io::Write,
) -> io::Result<Outcome> {
    let cap = capture(reader)?;
    report(&cap, policy, out, stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_wire::frame::{encode, MAX_FRAME};
    use std::time::Duration;

    fn frame(seq: u32, body: &str) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let e = encode(
            asperitas_wire::Level::Info,
            seq,
            seq * 10,
            body.as_bytes(),
            &mut buf,
        );
        buf[..e.len].to_vec()
    }

    fn status(dropped: u32) -> String {
        format!("STATUS proto=1 sent=5 dropped_full={dropped} bytes_dropped=0 trunc=0 ep_err=0 seq_next=9 pipe_free=100")
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn names() {
        assert!(is_console_name("cu.usbmodem1101"));
        assert!(is_console_name("ttyACM0"));
        assert!(!is_console_name("tty.usbmodem1101"));
        assert!(!is_console_name("ttyUSB0"));
    }

    #[test]
    fn pick() {
        assert_eq!(
            pick_device(Some(p("/x")), vec![p("/a"), p("/b")]),
            Ok(p("/x"))
        );
        assert_eq!(pick_device(None, vec![p("/a")]), Ok(p("/a")));
        assert_eq!(pick_device(None, vec![]), Err(DeviceError::None));
        let e = pick_device(None, vec![p("/a"), p("/b")]).unwrap_err();
        assert!(e.to_string().contains("/a") && e.to_string().contains("/b"));
    }

    #[test]
    fn list_filters_dir() {
        let d = tempfile::tempdir().unwrap();
        for n in ["ttyACM1", "cu.usbmodem9", "tty.usbmodem9", "ttyS0"] {
            std::fs::write(d.path().join(n), b"").unwrap();
        }
        let got = list_candidates(d.path()).unwrap();
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["cu.usbmodem9", "ttyACM1"]);
    }

    /// Yields timeouts before and between data, then blocks forever (timeouts).
    struct Flaky {
        script: Vec<io::Result<Vec<u8>>>,
    }
    impl Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.script.is_empty() {
                std::thread::sleep(Duration::from_millis(5));
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            match self.script.remove(0) {
                Ok(v) => {
                    buf[..v.len()].copy_from_slice(&v);
                    Ok(v.len())
                }
                Err(e) => Err(e),
            }
        }
    }

    #[test]
    fn deadline_survives_timeouts_and_stops_when_idle() {
        let mut data = frame(0, "BOOT proto=1 fw=x");
        data.extend(frame(1, &status(0)));
        let r = Flaky {
            script: vec![
                Err(io::Error::from(io::ErrorKind::TimedOut)),
                Ok(data),
                Err(io::Error::from(io::ErrorKind::TimedOut)),
            ],
        };
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("cap.txt");
        let mut sink = Vec::new();
        let t = Instant::now();
        let o = run(
            Deadline::new(r, Instant::now() + Duration::from_millis(100)),
            &Policy::default(),
            &out,
            &mut sink,
        )
        .unwrap();
        assert!(t.elapsed() >= Duration::from_millis(100));
        assert_eq!(o, Outcome::Pass);
        assert!(String::from_utf8(sink).unwrap().contains("verdict=pass"));
        assert!(std::fs::read_to_string(out).unwrap().contains("--\n"));
    }

    #[test]
    fn clean_stream_exits_zero() {
        let mut b = frame(0, "hi");
        b.extend(frame(1, &status(0)));
        let d = tempfile::tempdir().unwrap();
        let o = run(
            &b[..],
            &Policy::default(),
            &d.path().join("o"),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(o.code(), 0);
    }

    #[test]
    fn lossy_streams_exit_nonzero() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("o");
        // Device-reported drops over the limit.
        let mut b = frame(0, "hi");
        b.extend(frame(1, &status(3)));
        let o = run(&b[..], &Policy::default(), &out, &mut Vec::new()).unwrap();
        assert_eq!(o, Outcome::Fail);
        assert_eq!(o.code(), 1);
        // ... but within an allowed budget.
        let policy = Policy {
            max_device_dropped: 3,
        };
        assert_eq!(
            run(&b[..], &policy, &out, &mut Vec::new()).unwrap(),
            Outcome::Pass
        );
        // Corrupted frame.
        let mut c = [frame(0, "a"), frame(1, "bbbbbb")].concat();
        let n = frame(0, "a").len();
        c[n + 25] ^= 1;
        assert_eq!(
            run(&c[..], &Policy::default(), &out, &mut Vec::new()).unwrap(),
            Outcome::Fail
        );
    }

    #[test]
    fn read_error_is_error_not_verdict() {
        let r = Flaky {
            script: vec![Err(io::Error::other("unplugged"))],
        };
        let d = tempfile::tempdir().unwrap();
        assert!(run(r, &Policy::default(), &d.path().join("o"), &mut Vec::new()).is_err());
    }
}
