//! `asperitas-cli pitch` and `pitch-corpus`: run the pitch tracker over a WAV or a synthetic
//! signal, score it against an offline reference or exact truth, and write the estimate against
//! time as CSV and a PNG plot. See `docs/reference/pitch-accuracy.md`.

use crate::plot::{Canvas, AXIS, GRID, PLOT_W};
use crate::wav_io::read_wav;
use asperitas_measure::pitch_accuracy::{
    cents, classify, detect_onsets, reference_track, run_tracker, score, synthetic_noise,
    synthetic_notes, synthetic_sweep, Kind, Outcome, RefFrame, ScoreConfig, Scores, TrackFrame,
};
use std::fmt::Write as _;
use std::path::Path;

/// Prefix that selects a built-in signal instead of a WAV path.
const SYNTHETIC_PREFIX: &str = "synthetic:";
/// The built-in signals, in table order.
pub const SYNTHETIC_NAMES: [&str; 3] = ["sweep", "notes", "noise"];
/// Rate every built-in signal is generated at, and the rate the tracker is designed for.
const SYNTHETIC_RATE: f32 = 48_000.0;

const PANEL_H: usize = 280;
const MARGIN: usize = 8;
/// Frequency range of the estimate panel.
const PLOT_MIN_HZ: f64 = 30.0;
const PLOT_MAX_HZ: f64 = 2_400.0;
const VOICED_RGB: [u8; 3] = [226, 241, 226];
const REF_RGB: [u8; 3] = [60, 60, 60];
const EST_RGB: [u8; 3] = [200, 50, 40];
const CONF_RGB: [u8; 3] = [40, 90, 200];
const THRESH_RGB: [u8; 3] = [230, 140, 30];

pub struct PitchArgs {
    /// A WAV path, or `synthetic:sweep`, `synthetic:notes` or `synthetic:noise`.
    pub input: String,
    /// Output files are `<prefix>.csv` and `<prefix>.png`.
    pub output_prefix: String,
    /// Score confidence only: the signal is a chord, with no single right pitch.
    pub chordal: bool,
    /// The pitch the clip is meant to hold, for a sanity check of the reference; it never
    /// replaces the reference.
    pub nominal_hz: Option<f64>,
    pub clarity_threshold: f32,
    pub tolerance_cents: f64,
}

/// One analysed clip.
pub struct Clip {
    pub name: String,
    pub track: Vec<TrackFrame>,
    pub truth: Vec<RefFrame>,
    pub scores: Scores,
    pub config: ScoreConfig,
    /// How the truth was obtained, for the table.
    pub reference: String,
    /// Median offset of the offline reference from the nominal pitch, in cents.
    pub nominal_offset_cents: Option<f64>,
}

fn check_flags(clarity: f32, tolerance_cents: f64) -> Result<(), String> {
    if !(0.5..=0.99).contains(&clarity) {
        return Err(format!(
            "clarity_threshold out of range: {clarity} (allowed 0.5..=0.99)"
        ));
    }
    if !(tolerance_cents > 0.0 && tolerance_cents <= 600.0) {
        return Err(format!(
            "tolerance_cents out of range: {tolerance_cents} (allowed above 0 up to 600)"
        ));
    }
    Ok(())
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(|a, b| a.total_cmp(b));
    (!v.is_empty()).then(|| v[v.len() / 2])
}

/// Run one clip: load it, track it, build its truth and score it.
pub fn analyze_clip(args: &PitchArgs) -> Result<Clip, String> {
    check_flags(args.clarity_threshold, args.tolerance_cents)?;
    let config = ScoreConfig {
        tolerance_cents: args.tolerance_cents,
        clarity_threshold: f64::from(args.clarity_threshold),
    };
    let (name, samples, rate, truth, kind, reference) =
        if let Some(which) = args.input.strip_prefix(SYNTHETIC_PREFIX) {
            let s = match which {
                "sweep" => synthetic_sweep(SYNTHETIC_RATE),
                "notes" => synthetic_notes(SYNTHETIC_RATE),
                "noise" => synthetic_noise(SYNTHETIC_RATE),
                other => {
                    return Err(format!(
                        "unknown synthetic signal '{other}' (one of {})",
                        SYNTHETIC_NAMES.join(", ")
                    ))
                }
            };
            (
                which.to_string(),
                s.samples,
                SYNTHETIC_RATE,
                s.truth,
                s.kind,
                "exact (generator)".to_string(),
            )
        } else {
            let (spec, frames) = read_wav(&args.input)?;
            if !(8_000..=384_000).contains(&spec.sample_rate) {
                return Err(format!(
                    "sample rate out of range: {} (allowed 8000..=384000 Hz)",
                    spec.sample_rate
                ));
            }
            let samples: Vec<f32> = frames.iter().map(|f| 0.5 * (f[0] + f[1])).collect();
            if samples.len() < 2 * asperitas_measure::pitch_accuracy::HOP {
                return Err(format!("'{}' is too short to analyse", args.input));
            }
            let rate = spec.sample_rate as f32;
            let name = Path::new(&args.input)
                .file_stem()
                .map_or_else(|| args.input.clone(), |s| s.to_string_lossy().into_owned());
            let kind = if args.chordal {
                Kind::Chordal
            } else {
                Kind::Monophonic
            };
            (
                name,
                samples.clone(),
                rate,
                reference_track(&samples, rate),
                kind,
                "offline 4096-sample NSDF".to_string(),
            )
        };

    let track = run_tracker(&samples, rate, args.clarity_threshold);
    let onsets = detect_onsets(&samples, rate);
    let scores = score(&track, &truth, &onsets, kind, &config);
    let nominal_offset_cents = args.nominal_hz.and_then(|nominal| {
        median(
            truth
                .iter()
                .filter(|f| f.voiced)
                .map(|f| cents(f.hz, nominal))
                .collect(),
        )
    });
    let mut reference = reference;
    if kind == Kind::Chordal {
        reference.push_str(" (confidence only)");
    }
    if let (Some(nominal), Some(off)) = (args.nominal_hz, nominal_offset_cents) {
        let _ = write!(reference, "; {off:+.0} c from nominal {nominal} Hz");
    }
    Ok(Clip {
        name,
        track,
        truth,
        scores,
        config,
        reference,
        nominal_offset_cents,
    })
}

fn csv(clip: &Clip) -> String {
    let mut s = String::from(
        "t_s,estimate_hz,confidence,reference_hz,reference_voiced,level_db,error_cents\n",
    );
    for (t, r) in clip.track.iter().zip(&clip.truth) {
        let err = if r.voiced && t.hz > 0.0 {
            format!("{:.9e}", cents(t.hz, r.hz))
        } else {
            String::new()
        };
        let _ = writeln!(
            s,
            "{:.9e},{:.9e},{:.9e},{:.9e},{},{:.9e},{err}",
            t.t_s,
            t.hz,
            t.conf,
            r.hz,
            u8::from(r.voiced),
            r.level_db
        );
    }
    s
}

/// Top panel: the estimate in red and the reference in dark grey on a log frequency axis from
/// 30 Hz to 2.4 kHz with a line at every A (55, 110, ... 1760 Hz), voiced time shaded green.
/// Bottom panel: confidence in blue from 0 to 1 with the clarity threshold in orange. Time runs
/// from 0 to the last frame across both.
fn plot(clip: &Clip) -> Result<Vec<u8>, String> {
    let h = 2 * PANEL_H;
    let mut c = Canvas::new(h);
    let inner = PANEL_H - 2 * MARGIN;
    let n = clip.track.len().max(1);
    let t_end = clip.track.last().map_or(1.0, |f| f.t_s);
    let x_of = |t: f64| ((t / t_end) * (PLOT_W - 1) as f64).round() as usize;
    let y_hz = |hz: f64| {
        let t = ((hz / PLOT_MIN_HZ).ln() / (PLOT_MAX_HZ / PLOT_MIN_HZ).ln()).clamp(0.0, 1.0);
        MARGIN + ((1.0 - t) * (inner - 1) as f64).round() as usize
    };
    let y_conf = |v: f64| {
        PANEL_H + MARGIN + ((1.0 - v.clamp(0.0, 1.0)) * (inner - 1) as f64).round() as usize
    };

    for (k, r) in clip.truth.iter().enumerate().take(n) {
        if r.voiced {
            let x0 = x_of(r.t_s);
            let x1 = x_of(clip.track.get(k + 1).map_or(r.t_s, |f| f.t_s)).max(x0);
            for x in x0..=x1 {
                c.vline(x, MARGIN, MARGIN + inner - 1, VOICED_RGB);
                c.vline(
                    x,
                    PANEL_H + MARGIN,
                    PANEL_H + MARGIN + inner - 1,
                    VOICED_RGB,
                );
            }
        }
    }
    let mut a = 55.0;
    while a < PLOT_MAX_HZ {
        c.hline(y_hz(a), GRID);
        a *= 2.0;
    }
    for v in [0.0, 0.5, 1.0] {
        c.hline(y_conf(v), GRID);
    }
    c.hline(PANEL_H - 1, AXIS);
    c.hline(y_conf(clip.config.clarity_threshold), THRESH_RGB);
    for (t, r) in clip.track.iter().zip(&clip.truth) {
        let x = x_of(t.t_s).min(PLOT_W - 2);
        if r.voiced {
            c.dot(x, y_hz(r.hz).min(PANEL_H - 3), REF_RGB);
        }
        if t.hz > 0.0 {
            c.dot(x, y_hz(t.hz).min(PANEL_H - 3), EST_RGB);
        }
        c.dot(x, y_conf(t.conf).min(h - 3), CONF_RGB);
    }
    c.encode_png()
}

fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = Path::new(path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create '{}': {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("cannot write '{path}': {e}"))
}

fn write_clip(clip: &Clip, prefix: &str) -> Result<(), String> {
    write_file(&format!("{prefix}.csv"), csv(clip).as_bytes())?;
    write_file(&format!("{prefix}.png"), &plot(clip)?)
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "-".into(), |v| format!("{:.1}%", 100.0 * v))
}

fn num(v: Option<f64>, decimals: usize) -> String {
    v.map_or_else(|| "-".into(), |v| format!("{v:.decimals$}"))
}

/// Header of the Markdown table, and the divider line.
pub fn table_header() -> String {
    "| Clip | Voiced s | Correct | Within 10 c | Octave | Other harmonic | Other wrong | No estimate | Unvoiced conf mean / max | Unvoiced confident | Onsets (missed) | Latency median / max ms | Reference |\n\
     |---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|---|\n"
        .to_string()
}

/// One Markdown table row.
pub fn table_row(clip: &Clip) -> String {
    let s = &clip.scores;
    let voiced = if s.kind == Kind::Chordal {
        "-".to_string()
    } else {
        format!("{:.2}", s.voiced_s)
    };
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} / {} | {} | {} ({}) | {} / {} | {} |\n",
        clip.name,
        voiced,
        pct(s.correct),
        pct(s.within_10c),
        pct(s.octave_error),
        pct(s.harmonic_error),
        pct(s.other_error),
        pct(s.no_estimate),
        num(s.unvoiced_conf_mean, 2),
        num(s.unvoiced_conf_max, 2),
        pct(s.unvoiced_confident),
        s.onsets,
        s.onsets_missed,
        num(s.latency_median_ms, 0),
        num(s.latency_max_ms, 0),
        clip.reference
    )
}

/// Reference-pitch bands of [`band_table`], in Hz: lower bound inclusive, upper exclusive.
const BANDS: [(f64, f64); 6] = [
    (0.0, 60.0),
    (60.0, 120.0),
    (120.0, 250.0),
    (250.0, 500.0),
    (500.0, 1000.0),
    (1000.0, f64::INFINITY),
];

/// Markdown table of the fraction of voiced frames with a correct estimate, by band of reference
/// pitch, for every monophonic clip. A cell is `-` where the clip has no voiced frame in the
/// band, and carries the frame count so a thin cell reads as thin.
pub fn band_table(clips: &[Clip]) -> String {
    let mut s = String::from("| Clip |");
    let mut rule = String::from("|---|");
    for (lo, hi) in BANDS {
        if hi.is_finite() {
            let _ = write!(s, " {lo:.0}-{hi:.0} Hz |");
        } else {
            let _ = write!(s, " {lo:.0}+ Hz |");
        }
        rule.push_str("--:|");
    }
    s.push('\n');
    s.push_str(&rule);
    s.push('\n');
    for clip in clips.iter().filter(|c| c.scores.kind == Kind::Monophonic) {
        let _ = write!(s, "| {} |", clip.name);
        for (lo, hi) in BANDS {
            let (mut n, mut ok) = (0usize, 0usize);
            for (t, r) in clip.track.iter().zip(&clip.truth) {
                if r.voiced && r.hz >= lo && r.hz < hi {
                    n += 1;
                    ok += usize::from(
                        classify(t.hz, r.hz, clip.config.tolerance_cents) == Outcome::Correct,
                    );
                }
            }
            if n == 0 {
                s.push_str(" - |");
            } else {
                let _ = write!(s, " {:.1}% ({n}) |", 100.0 * ok as f64 / n as f64);
            }
        }
        s.push('\n');
    }
    s
}

/// One-line score summary for stdout.
pub fn summary(clip: &Clip) -> String {
    table_header() + &table_row(clip)
}

/// Run one clip and write `<prefix>.csv` and `<prefix>.png`; returns the table for it.
pub fn run_pitch(args: &PitchArgs) -> Result<String, String> {
    let clip = analyze_clip(args)?;
    write_clip(&clip, &args.output_prefix)?;
    Ok(summary(&clip))
}

pub struct CorpusArgs {
    pub corpus_dir: String,
    pub output_dir: String,
    pub clarity_threshold: f32,
    pub tolerance_cents: f64,
}

/// Run every WAV in the corpus directory (in name order) and the built-in signals, write
/// `<name>.csv` and `<name>.png` for each and `table.md` under the output directory, and return
/// the table. A clip with `chord` in its name is scored on confidence only; one with
/// `single_note` in its name is checked against the open A of its instrument (220 Hz for an
/// octave mandolin, 440 Hz otherwise).
pub fn run_corpus(args: &CorpusArgs) -> Result<String, String> {
    check_flags(args.clarity_threshold, args.tolerance_cents)?;
    let mut wavs: Vec<_> = std::fs::read_dir(&args.corpus_dir)
        .map_err(|e| format!("cannot read '{}': {e}", args.corpus_dir))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    wavs.sort();
    if wavs.is_empty() {
        return Err(format!("no .wav files in '{}'", args.corpus_dir));
    }
    let mut inputs: Vec<String> = wavs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    inputs.extend(
        SYNTHETIC_NAMES
            .iter()
            .map(|n| format!("{SYNTHETIC_PREFIX}{n}")),
    );

    let mut table = table_header();
    let mut clips = Vec::new();
    for input in inputs {
        let name = input.rsplit('/').next().unwrap_or(&input);
        let nominal_hz = name.contains("single_note").then(|| {
            if name.starts_with("octave_") {
                220.0
            } else {
                440.0
            }
        });
        let pa = PitchArgs {
            input: input.clone(),
            output_prefix: String::new(),
            chordal: name.contains("chord"),
            nominal_hz,
            clarity_threshold: args.clarity_threshold,
            tolerance_cents: args.tolerance_cents,
        };
        let clip = analyze_clip(&pa)?;
        write_clip(&clip, &format!("{}/{}", args.output_dir, clip.name))?;
        table.push_str(&table_row(&clip));
        clips.push(clip);
    }
    table.push_str("\nCorrect fraction by reference pitch (voiced frames in band):\n\n");
    table.push_str(&band_table(&clips));
    write_file(&format!("{}/table.md", args.output_dir), table.as_bytes())?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(input: &str, prefix: &str) -> PitchArgs {
        PitchArgs {
            input: input.into(),
            output_prefix: prefix.into(),
            chordal: false,
            nominal_hz: None,
            clarity_threshold: 0.8,
            tolerance_cents: 50.0,
        }
    }

    #[test]
    fn writes_csv_and_png_identically_twice() {
        let dir = tempfile::tempdir().unwrap();
        let go = |name: &str| {
            let prefix = dir.path().join(name).to_str().unwrap().to_string();
            let table = run_pitch(&args("synthetic:notes", &prefix)).unwrap();
            let read = |s: &str| std::fs::read(format!("{prefix}{s}")).unwrap();
            (table, read(".csv"), read(".png"))
        };
        let a = go("a");
        let b = go("b");
        assert_eq!(a, b);
        assert!(a.2.starts_with(b"\x89PNG"));
        let csv = String::from_utf8(a.1).unwrap();
        assert!(csv.starts_with("t_s,estimate_hz,confidence,reference_hz"));
        assert_eq!(csv.lines().count(), 1 + 12 * 48_000 / 2 / 128);
    }

    #[test]
    fn bad_input_is_refused_naming_it() {
        let e = |a: PitchArgs| analyze_clip(&a).err().unwrap();
        assert!(e(args("synthetic:bogus", "x")).contains("bogus"));
        assert!(e(args("/nonexistent/x.wav", "x")).contains("cannot open"));
        let mut a = args("synthetic:notes", "x");
        a.clarity_threshold = 2.0;
        assert!(e(a).contains("clarity_threshold"));
        let mut a = args("synthetic:notes", "x");
        a.tolerance_cents = -1.0;
        assert!(e(a).contains("tolerance_cents"));
    }

    #[test]
    fn wav_round_trip_scores_a_generated_tone() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("tone.wav");
        let frames: Vec<[f32; 2]> = (0..48_000 * 2)
            .map(|i| {
                let v = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin();
                [v, v]
            })
            .collect();
        crate::wav_io::write_wav(wav.to_str().unwrap(), 48_000, &frames).unwrap();
        let mut a = args(wav.to_str().unwrap(), "x");
        a.nominal_hz = Some(220.0);
        let clip = analyze_clip(&a).unwrap();
        assert!(clip.scores.correct.unwrap() > 0.9, "{:?}", clip.scores);
        assert!(clip.nominal_offset_cents.unwrap().abs() < 2.0);
        assert_eq!(clip.name, "tone");
    }
}
