//! The sidecar that travels with a recorded rig session.
//!
//! A raw console stream holds what the device said, but not what the host asked for or how the
//! knob was driven: the effect selection, which knob moved, and the schedule's shape live only on
//! the host. [`Sidecar`] records them as plain `key=value` lines next to the stream, so an analysis
//! reads one file instead of repeating the session's flags. The device times it can supply itself
//! (`arm_t_ms`, `first_set_t_ms`) are copied from the stream by the recorder and are optional.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

const MAGIC: &str = "asperitas-session v1";

/// How the knob was driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Linear ramp from `from` to `to` over `duration_s`.
    Sweep,
    /// Long triangle ramp with optional snaps.
    Soak,
    /// One level held for the whole session.
    Hold,
}

impl Mode {
    fn token(self) -> &'static str {
        match self {
            Mode::Sweep => "sweep",
            Mode::Soak => "soak",
            Mode::Hold => "hold",
        }
    }

    fn from_token(s: &str) -> Option<Self> {
        [Mode::Sweep, Mode::Soak, Mode::Hold]
            .into_iter()
            .find(|m| m.token() == s)
    }
}

/// What the host did during a session. Times are seconds after the capture armed.
#[derive(Clone, Debug, PartialEq)]
pub struct Sidecar {
    /// Effect selection sent with `CTL FX`.
    pub fx_f: u8,
    /// Value sent with `CTL FX`.
    pub fx_v: u16,
    /// Knob id the schedule drove.
    pub knob: u8,
    pub mode: Mode,
    /// Knob level at the start of the motion, `0..=1`.
    pub from: f64,
    /// Knob level at the end of the motion; equal to `from` for a hold.
    pub to: f64,
    /// Length of the motion.
    pub duration_s: f64,
    /// Delay between the capture arming and the first scheduled `SET`.
    pub start_after_arm_s: f64,
    /// Soak only: start times of the snaps, in seconds from the start of the motion.
    pub snaps: Vec<f64>,
    /// Soak only: level added during a snap.
    pub snap_level: f64,
    /// Device `t_ms` of the first status record showing the capture armed, as the stream gave it.
    pub arm_t_ms: Option<u32>,
    /// Device `t_ms` of the first `CTLR OK SET`, as the stream gave it.
    pub first_set_t_ms: Option<u32>,
}

/// Where the sidecar for the stream at `out` lives: `out` with `.session` appended.
pub fn sidecar_path(out: &Path) -> PathBuf {
    let mut name: OsString = out.as_os_str().to_owned();
    name.push(".session");
    PathBuf::from(name)
}

/// Device `t_ms` is transmitted modulo this many milliseconds.
const T_MS_MODULUS: i64 = 100_000_000;

/// Why a sidecar did not parse.
#[derive(Debug, PartialEq, Eq)]
pub struct SidecarError(pub String);

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad session sidecar: {}", self.0)
    }
}

impl std::error::Error for SidecarError {}

impl Sidecar {
    /// Seconds from the capture arming to the first scheduled `SET`. Prefers the device clock
    /// (first `SET` reply minus the arming status record, wrapped as the stream wraps), which
    /// includes real scheduling latency; falls back to the host's planned delay when the recorder
    /// could not read one of the two times.
    pub fn sweep_start_s(&self) -> f64 {
        match (self.arm_t_ms, self.first_set_t_ms) {
            (Some(arm), Some(set)) => {
                (i64::from(set) - i64::from(arm)).rem_euclid(T_MS_MODULUS) as f64 / 1000.0
            }
            _ => self.start_after_arm_s,
        }
    }

    /// Render as `key=value` lines after a version line.
    pub fn render(&self) -> String {
        let snaps: Vec<String> = self.snaps.iter().map(f64::to_string).collect();
        let mut s = format!(
            "{MAGIC}\nfx_f={}\nfx_v={}\nknob={}\nmode={}\nfrom={}\nto={}\nduration_s={}\n\
             start_after_arm_s={}\nsnaps={}\nsnap_level={}\n",
            self.fx_f,
            self.fx_v,
            self.knob,
            self.mode.token(),
            self.from,
            self.to,
            self.duration_s,
            self.start_after_arm_s,
            snaps.join(","),
            self.snap_level,
        );
        if let Some(t) = self.arm_t_ms {
            s.push_str(&format!("arm_t_ms={t}\n"));
        }
        if let Some(t) = self.first_set_t_ms {
            s.push_str(&format!("first_set_t_ms={t}\n"));
        }
        s
    }

    /// Parse what [`render`](Self::render) wrote. Unknown keys and missing required keys are errors.
    pub fn parse(text: &str) -> Result<Self, SidecarError> {
        let mut lines = text.lines();
        if lines.next() != Some(MAGIC) {
            return Err(SidecarError(format!("first line is not {MAGIC:?}")));
        }
        let mut kv = std::collections::BTreeMap::new();
        for line in lines.filter(|l| !l.trim().is_empty()) {
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| SidecarError(format!("line without '=': {line:?}")))?;
            kv.insert(k.to_string(), v.to_string());
        }
        let mut take = |key: &str| -> Result<String, SidecarError> {
            kv.remove(key)
                .ok_or_else(|| SidecarError(format!("missing {key}")))
        };
        fn num<T: std::str::FromStr>(key: &str, v: &str) -> Result<T, SidecarError> {
            v.parse()
                .map_err(|_| SidecarError(format!("{key}={v:?} is not a valid number")))
        }
        let fx_f = num("fx_f", &take("fx_f")?)?;
        let fx_v = num("fx_v", &take("fx_v")?)?;
        let knob = num("knob", &take("knob")?)?;
        let mode_text = take("mode")?;
        let mode = Mode::from_token(&mode_text)
            .ok_or_else(|| SidecarError(format!("unknown mode {mode_text:?}")))?;
        let from = num("from", &take("from")?)?;
        let to = num("to", &take("to")?)?;
        let duration_s = num("duration_s", &take("duration_s")?)?;
        let start_after_arm_s = num("start_after_arm_s", &take("start_after_arm_s")?)?;
        let snaps_text = take("snaps")?;
        let snaps = if snaps_text.is_empty() {
            Vec::new()
        } else {
            snaps_text
                .split(',')
                .map(|s| num("snaps", s))
                .collect::<Result<_, _>>()?
        };
        let snap_level = num("snap_level", &take("snap_level")?)?;
        let arm_t_ms = kv
            .remove("arm_t_ms")
            .map(|v| num("arm_t_ms", &v))
            .transpose()?;
        let first_set_t_ms = kv
            .remove("first_set_t_ms")
            .map(|v| num("first_set_t_ms", &v))
            .transpose()?;
        if let Some(k) = kv.keys().next() {
            return Err(SidecarError(format!("unknown key {k:?}")));
        }
        Ok(Self {
            fx_f,
            fx_v,
            knob,
            mode,
            from,
            to,
            duration_s,
            start_after_arm_s,
            snaps,
            snap_level,
            arm_t_ms,
            first_set_t_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sidecar {
        Sidecar {
            fx_f: 1,
            fx_v: 0,
            knob: 0,
            mode: Mode::Soak,
            from: 0.5,
            to: 0.95,
            duration_s: 250.0,
            start_after_arm_s: 10.0,
            snaps: vec![80.0, 160.0],
            snap_level: 0.25,
            arm_t_ms: Some(4210),
            first_set_t_ms: None,
        }
    }

    #[test]
    fn round_trips() {
        let s = sample();
        assert_eq!(Sidecar::parse(&s.render()), Ok(s));
        let h = Sidecar {
            mode: Mode::Hold,
            snaps: vec![],
            arm_t_ms: None,
            first_set_t_ms: Some(77),
            ..sample()
        };
        assert_eq!(Sidecar::parse(&h.render()), Ok(h));
    }

    #[test]
    fn sweep_start_prefers_the_device_clock_and_wraps() {
        let s = Sidecar {
            arm_t_ms: Some(4210),
            first_set_t_ms: Some(14_710),
            ..sample()
        };
        assert_eq!(s.sweep_start_s(), 10.5);
        let wrapped = Sidecar {
            arm_t_ms: Some(99_999_000),
            first_set_t_ms: Some(9_500),
            ..sample()
        };
        assert_eq!(wrapped.sweep_start_s(), 10.5);
        let planned = Sidecar {
            first_set_t_ms: None,
            ..sample()
        };
        assert_eq!(planned.sweep_start_s(), 10.0);
    }

    #[test]
    fn sidecar_path_appends_session() {
        assert_eq!(
            sidecar_path(Path::new("a/b.console.zst")),
            PathBuf::from("a/b.console.zst.session")
        );
    }

    #[test]
    fn rejects_malformed() {
        assert!(Sidecar::parse("nope").is_err());
        let ok = sample().render();
        assert!(Sidecar::parse(&ok.replace("knob=0", "knob=x")).is_err());
        assert!(Sidecar::parse(&ok.replace("mode=soak", "mode=zig")).is_err());
        assert!(Sidecar::parse(&format!("{ok}extra=1\n")).is_err());
        assert!(Sidecar::parse(&ok.replace("fx_f=1\n", "")).is_err());
    }
}
