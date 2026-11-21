//! `asperitas-cli trace`: the internal signals of the resonator render of a clip over time, as CSV
//! and a PNG plot, so a modulated or tracked render can be seen as well as heard. The render is
//! the one `process --processor resonator` runs, so a trace and a render of the same arguments
//! agree. See `docs/reference/analyze.md`.

use crate::plot::{Canvas, AXIS, GRID, PLOT_W};
use crate::process::{parse_param_pairs, ResonatorDrive};
use crate::wav_io;
use asperitas_dsp::asperitas::BLOCK_FRAMES;
use asperitas_dsp::root_follow::CONFIDENCE_THRESHOLD;
use asperitas_dsp::tracked_resonator::Trace;
use std::fmt::Write as _;
use std::path::Path;

const PANEL_H: usize = 200;
const MARGIN: usize = 8;
const RED: [u8; 3] = [200, 50, 40];
const BLUE: [u8; 3] = [40, 90, 200];

pub struct TraceArgs {
    pub input: String,
    /// Output files are `<prefix>_trace.csv` and `<prefix>_trace.png`.
    pub output_prefix: String,
    /// `key=value` resonator settings, as `process --params`.
    pub params: Vec<String>,
    /// Knob automation file, as `process --automation`.
    pub automation: Option<String>,
}

/// One row per [`BLOCK_FRAMES`]-frame block, taken at the end of the block.
pub struct TraceRun {
    pub sample_rate: u32,
    pub rows: Vec<Trace>,
}

/// Render the clip through the resonator stage and record the stage's [`Trace`] after each block.
pub fn trace_clip(args: &TraceArgs) -> Result<TraceRun, String> {
    let (spec, mut frames) = wav_io::read_wav(&args.input)?;
    let pairs = parse_param_pairs(&args.params)?;
    let drive = ResonatorDrive::load(&pairs, args.automation.as_deref(), spec.sample_rate)?;
    let mut rows = Vec::with_capacity(frames.len() / BLOCK_FRAMES + 1);
    drive.render(spec.sample_rate as f32, &mut frames, |stage| {
        rows.push(stage.trace())
    });
    Ok(TraceRun {
        sample_rate: spec.sample_rate,
        rows,
    })
}

fn csv(run: &TraceRun) -> String {
    let mut s = String::from("block,t_s,follower,walk,estimate_hz,confidence,root_hz\n");
    for (i, r) in run.rows.iter().enumerate() {
        // The time of the block's last frame boundary: the end of block `i`.
        let t = ((i + 1) * BLOCK_FRAMES) as f64 / f64::from(run.sample_rate);
        let _ = writeln!(
            s,
            "{i},{t:.9e},{:.9e},{:.9e},{:.9e},{:.9e},{:.9e}",
            r.follower, r.walk, r.estimate_hz, r.confidence, r.effective_root_hz
        );
    }
    s
}

/// Three stacked panels over the clip's duration (left edge to right edge):
/// 1. follower (red) and random walk (blue) on one scale, -1 at the bottom to 1 at the top, zero
///    line in the middle;
/// 2. confidence (red), 0 at the bottom to 1 at the top, with a line at the root-follow
///    threshold;
/// 3. the tracker's estimate (blue, absent while it is 0) and the effective root (red) on a log
///    frequency axis spanning the octaves they use, a line at each octave.
pub(crate) fn plot_png(run: &TraceRun) -> Result<Vec<u8>, String> {
    let mut c = Canvas::new(3 * PANEL_H);
    let inner = PANEL_H - 2 * MARGIN;
    let n = run.rows.len();
    let top = |panel: usize| panel * PANEL_H + MARGIN;
    let row_at = |x: usize| (x * n / PLOT_W).min(n.saturating_sub(1));

    // Draws `value(row)` as a connected stroke, one point per column; `None` breaks the stroke.
    let stroke = |c: &mut Canvas, rgb: [u8; 3], value: &dyn Fn(&Trace) -> Option<usize>| {
        if n == 0 {
            return;
        }
        let mut prev: Option<usize> = None;
        for x in 0..PLOT_W {
            match value(&run.rows[row_at(x)]) {
                Some(y) => {
                    c.vline(x, prev.unwrap_or(y), y, rgb);
                    prev = Some(y);
                }
                None => prev = None,
            }
        }
    };
    let y_unit = |panel: usize, t: f32| {
        top(panel) + ((1.0 - t.clamp(0.0, 1.0)) * (inner - 1) as f32).round() as usize
    };

    // Panel 1.
    c.hline(y_unit(0, 0.5), AXIS);
    stroke(&mut c, RED, &|r| Some(y_unit(0, 0.5 + 0.5 * r.follower)));
    stroke(&mut c, BLUE, &|r| Some(y_unit(0, 0.5 + 0.5 * r.walk)));

    // Panel 2.
    c.hline(y_unit(1, CONFIDENCE_THRESHOLD), GRID);
    c.hline(y_unit(1, 0.0), AXIS);
    stroke(&mut c, RED, &|r| Some(y_unit(1, r.confidence)));

    // Panel 3.
    let hz_values = |r: &Trace| [r.estimate_hz, r.effective_root_hz];
    let (mut lo, mut hi) = (f32::MAX, 0.0f32);
    for r in &run.rows {
        for hz in hz_values(r).into_iter().filter(|h| *h > 0.0) {
            lo = lo.min(hz);
            hi = hi.max(hz);
        }
    }
    if hi <= 0.0 {
        (lo, hi) = (110.0, 220.0);
    }
    let lo_oct = lo.log2().floor();
    let hi_oct = hi.log2().ceil().max(lo_oct + 1.0);
    let y_hz = |hz: f32| y_unit(2, (hz.log2() - lo_oct) / (hi_oct - lo_oct));
    for oct in 0..=(hi_oct - lo_oct) as usize {
        c.hline(y_hz(2f32.powf(lo_oct + oct as f32)), GRID);
    }
    stroke(&mut c, BLUE, &|r| {
        (r.estimate_hz > 0.0).then(|| y_hz(r.estimate_hz))
    });
    stroke(&mut c, RED, &|r| Some(y_hz(r.effective_root_hz)));

    c.encode_png()
}

/// Run the command: write `<prefix>_trace.csv` and `<prefix>_trace.png`.
pub fn run_trace(args: &TraceArgs) -> Result<(), String> {
    let run = trace_clip(args)?;
    let prefix = &args.output_prefix;
    let write = |suffix: &str, bytes: &[u8]| -> Result<(), String> {
        let path = format!("{prefix}{suffix}");
        if let Some(dir) = Path::new(&path)
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
        {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create '{}': {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("cannot write '{path}': {e}"))
    };
    write("_trace.csv", csv(&run).as_bytes())?;
    write("_trace.png", &plot_png(&run)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_dsp::processor::Frame;

    fn write_sine(path: &Path, hz: f32, secs: f32) {
        let frames: Vec<Frame> = (0..(48_000.0 * secs) as usize)
            .map(|n| {
                let v = 0.5 * (2.0 * std::f32::consts::PI * hz * n as f32 / 48_000.0).sin();
                [v, v]
            })
            .collect();
        wav_io::write_wav(path.to_str().unwrap(), 48_000, &frames).unwrap();
    }

    fn args(dir: &Path, name: &str, params: &[&str]) -> TraceArgs {
        TraceArgs {
            input: dir.join("sine.wav").to_str().unwrap().to_string(),
            output_prefix: dir.join(name).to_str().unwrap().to_string(),
            params: params.iter().map(|s| s.to_string()).collect(),
            automation: None,
        }
    }

    fn column(csv: &str, name: &str) -> Vec<f64> {
        let mut lines = csv.lines();
        let i = lines
            .next()
            .unwrap()
            .split(',')
            .position(|h| h == name)
            .unwrap();
        lines
            .map(|l| l.split(',').nth(i).unwrap().parse().unwrap())
            .collect()
    }

    #[test]
    fn constant_sine_is_tracked_and_the_csv_says_so() {
        let dir = tempfile::tempdir().unwrap();
        write_sine(&dir.path().join("sine.wav"), 220.0, 1.0);
        let a = args(
            dir.path(),
            "t",
            &["root_source=2", "follower_depth=0.5", "walk_depth=0.5"],
        );
        run_trace(&a).unwrap();
        let text = std::fs::read_to_string(dir.path().join("t_trace.csv")).unwrap();
        assert!(text.starts_with("block,t_s,follower,walk,estimate_hz,confidence,root_hz\n"));
        let rows = 48_000 / BLOCK_FRAMES;
        assert_eq!(text.lines().count(), rows + 1);
        assert_eq!(column(&text, "block")[rows - 1], (rows - 1) as f64);
        let t = column(&text, "t_s");
        assert!((t[0] - BLOCK_FRAMES as f64 / 48_000.0).abs() < 1e-9);

        let last = |name: &str| *column(&text, name).last().unwrap();
        assert!((last("estimate_hz") - 220.0).abs() < 2.0);
        assert!(last("confidence") > 0.8);
        assert!(
            (last("root_hz") - 220.0).abs() < 0.5,
            "root {}",
            last("root_hz")
        );
        assert!(last("follower") > 0.1);
        assert!(column(&text, "walk").iter().any(|w| *w != 0.0));
        let png = std::fs::read(dir.path().join("t_trace.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn manual_root_stays_put_and_output_is_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        write_sine(&dir.path().join("sine.wav"), 220.0, 0.25);
        let read = |name: &str| {
            run_trace(&args(dir.path(), name, &["root_hz=150"])).unwrap();
            (
                std::fs::read(dir.path().join(format!("{name}_trace.csv"))).unwrap(),
                std::fs::read(dir.path().join(format!("{name}_trace.png"))).unwrap(),
            )
        };
        let (a, b) = (read("a"), read("b"));
        assert_eq!(a, b);
        let text = String::from_utf8(a.0).unwrap();
        assert!(column(&text, "root_hz")
            .iter()
            .all(|r| (r - 150.0).abs() < 1e-3));
    }

    #[test]
    fn bad_arguments_are_refused_naming_them() {
        let dir = tempfile::tempdir().unwrap();
        write_sine(&dir.path().join("sine.wav"), 220.0, 0.1);
        for (param, key) in [
            ("bogus=1", "bogus"),
            ("follower_depth=2", "follower_depth"),
            ("root_source=3", "root_source"),
        ] {
            let err = run_trace(&args(dir.path(), "x", &[param])).unwrap_err();
            assert!(err.contains(key), "{param}: {err}");
        }
        let mut a = args(dir.path(), "x", &["root_hz=150"]);
        a.automation = Some("whatever.txt".into());
        assert!(run_trace(&a).unwrap_err().contains("--automation"));
    }
}
