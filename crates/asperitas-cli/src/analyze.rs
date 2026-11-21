//! `asperitas-cli analyze`: the resonator's impulse response and magnitude frequency response at a
//! given setting, as CSV and as a PNG plot, so tuning, ringing and the effect of damping, structure
//! and spread can be read without listening. See `docs/reference/analyze.md`.

use crate::plot::{Canvas, AXIS, GRID, PLOT_W};
use crate::process::parse_resonator_controls;
use asperitas_dsp::processor::Processor;
use asperitas_dsp::resonator_bank::{Resonator, MAX_PITCH_FRACTION, VOICE_COUNT};
use rustfft::{num_complex::Complex, FftPlanner};
use std::fmt::Write as _;
use std::path::Path;

/// Lowest frequency shown and searched for peaks.
const MIN_HZ: f64 = 20.0;
/// A peak must be within this many dB of the strongest one to be reported.
const PEAK_RANGE_DB: f64 = 50.0;
/// Magnitude floor so silence stays finite in dB.
const FLOOR: f64 = 1e-12;
/// Longest impulse response analysed, in seconds (the longest root T60).
const MAX_LENGTH_S: f32 = 10.0;

const PANEL_H: usize = 280;
const MARGIN: usize = 8;
/// Dynamic range of the frequency-response panel in dB.
const PLOT_DB_RANGE: f64 = 90.0;

pub struct AnalyzeArgs {
    /// `key=value` resonator settings, as `process --params`.
    pub params: Vec<String>,
    pub sample_rate: u32,
    /// Impulse length in seconds; `None` is the root T60.
    pub length_s: Option<f32>,
    /// Output files are `<prefix>_impulse.csv`, `<prefix>_response.csv` and `<prefix>.png`.
    pub output_prefix: String,
}

/// One detected spectral peak.
#[derive(Clone, Debug)]
pub struct Peak {
    pub freq_hz: f64,
    /// Level of the left and right mean power, left and right.
    pub db: f64,
    pub left_db: f64,
    pub right_db: f64,
    /// 1-based partial number this peak is nearest to, and that partial's expected pitch.
    pub partial: usize,
    pub expected_hz: f64,
}

#[derive(Debug)]
pub struct Analysis {
    pub sample_rate: f32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// Width of one response bin in Hz.
    pub bin_hz: f64,
    /// Magnitudes in dB for bins `0..=nfft/2`.
    pub left_db: Vec<f64>,
    pub right_db: Vec<f64>,
    pub peaks: Vec<Peak>,
}

fn db(mag: f64) -> f64 {
    20.0 * mag.max(FLOOR).log10()
}

/// Drive the bank with a unit impulse on both inputs and analyse what comes out. The impulse
/// passes through the excitation stage at its default voicing, which is transparent.
pub fn analyze(
    sample_rate: f32,
    params: &asperitas_dsp::resonator_bank::ResonatorParams,
    length_s: Option<f32>,
) -> Result<Analysis, String> {
    if !(8_000.0..=384_000.0).contains(&sample_rate) {
        return Err(format!(
            "sample_rate out of range: {sample_rate} (allowed 8000..=384000 Hz)"
        ));
    }
    let length_s = length_s.unwrap_or_else(|| params.root_decay_s().min(MAX_LENGTH_S));
    if !(length_s.is_finite() && (0.01..=MAX_LENGTH_S).contains(&length_s)) {
        return Err(format!(
            "length_s out of range: {length_s} (allowed 0.01..={MAX_LENGTH_S} s)"
        ));
    }
    let n = (length_s * sample_rate) as usize;

    let mut bank = Resonator::new(sample_rate, params);
    bank.snap_params();
    let mut frames = vec![[0.0f32; 2]; n];
    frames[0] = [1.0, 1.0];
    bank.process_block(&mut frames);
    let left: Vec<f32> = frames.iter().map(|f| f[0]).collect();
    let right: Vec<f32> = frames.iter().map(|f| f[1]).collect();

    let nfft = n.next_power_of_two();
    let spectrum = |x: &[f32]| -> Vec<Complex<f64>> {
        let mut buf: Vec<Complex<f64>> =
            x.iter().map(|&v| Complex::new(f64::from(v), 0.0)).collect();
        buf.resize(nfft, Complex::new(0.0, 0.0));
        FftPlanner::<f64>::new()
            .plan_fft_forward(nfft)
            .process(&mut buf);
        buf.truncate(nfft / 2 + 1);
        buf
    };
    let (sl, sr) = (spectrum(&left), spectrum(&right));
    let left_db: Vec<f64> = sl.iter().map(|c| db(c.norm())).collect();
    let right_db: Vec<f64> = sr.iter().map(|c| db(c.norm())).collect();
    let bin_hz = f64::from(sample_rate) / nfft as f64;

    let peaks = find_peaks(params, sample_rate, bin_hz, &sl, &sr);
    Ok(Analysis {
        sample_rate,
        left,
        right,
        bin_hz,
        left_db,
        right_db,
        peaks,
    })
}

/// Local maxima of the mean-power response, strongest within `PEAK_RANGE_DB`, refined by a
/// parabola through the three bins around each.
fn find_peaks(
    params: &asperitas_dsp::resonator_bank::ResonatorParams,
    sample_rate: f32,
    bin_hz: f64,
    sl: &[Complex<f64>],
    sr: &[Complex<f64>],
) -> Vec<Peak> {
    let power: Vec<f64> = sl
        .iter()
        .zip(sr)
        .map(|(l, r)| {
            10.0 * (0.5 * (l.norm_sqr() + r.norm_sqr()))
                .max(FLOOR * FLOOR)
                .log10()
        })
        .collect();
    let lo = (MIN_HZ / bin_hz).ceil() as usize;
    let top = power.iter().cloned().fold(f64::MIN, f64::max);
    // A peak must beat everything within a quarter of the root, so the ripple of a truncated
    // tail is not reported as a partial.
    let half = ((0.25 * f64::from(params.root_hz) / bin_hz).round() as usize).max(2);
    let expected: Vec<f64> = (0..VOICE_COUNT)
        .map(|i| f64::from(Resonator::partial_hz(params.root_hz, params.structure, i)))
        .filter(|&f| f <= f64::from(MAX_PITCH_FRACTION * sample_rate))
        .collect();

    let mut peaks = Vec::new();
    for k in lo.max(1)..power.len() - 1 {
        if power[k] < top - PEAK_RANGE_DB {
            continue;
        }
        let a = k.saturating_sub(half).max(1);
        let b = (k + half).min(power.len() - 1);
        if (a..=b).any(|j| power[j] > power[k] || (power[j] == power[k] && j < k)) {
            continue;
        }
        let (y0, y1, y2) = (power[k - 1], power[k], power[k + 1]);
        let denom = y0 - 2.0 * y1 + y2;
        let shift = if denom < 0.0 {
            0.5 * (y0 - y2) / denom
        } else {
            0.0
        };
        let freq_hz = (k as f64 + shift) * bin_hz;
        let Some((i, &expected_hz)) = expected.iter().enumerate().min_by(|x, y| {
            (x.1 - freq_hz)
                .abs()
                .partial_cmp(&(y.1 - freq_hz).abs())
                .unwrap()
        }) else {
            continue;
        };
        peaks.push(Peak {
            freq_hz,
            db: y1,
            left_db: db(sl[k].norm()),
            right_db: db(sr[k].norm()),
            partial: i + 1,
            expected_hz,
        });
    }
    peaks
}

fn impulse_csv(a: &Analysis) -> String {
    let mut s = String::from("n,t_s,left,right\n");
    for (i, (l, r)) in a.left.iter().zip(&a.right).enumerate() {
        let _ = writeln!(
            s,
            "{i},{:.9e},{:.9e},{:.9e}",
            i as f64 / f64::from(a.sample_rate),
            l,
            r
        );
    }
    s
}

fn response_csv(a: &Analysis) -> String {
    let mut s = String::from("freq_hz,left_db,right_db,diff_db\n");
    for (k, (l, r)) in a.left_db.iter().zip(&a.right_db).enumerate() {
        let _ = writeln!(
            s,
            "{:.9e},{:.9e},{:.9e},{:.9e}",
            k as f64 * a.bin_hz,
            l,
            r,
            l - r
        );
    }
    s
}

/// The peaks as text, one per line, for reading tuning without opening a file.
pub fn peaks_report(a: &Analysis) -> String {
    let mut s = String::from("partial expected_hz peak_hz cents db left_db right_db\n");
    for p in &a.peaks {
        let _ = writeln!(
            s,
            "{:>7} {:>11.2} {:>7.2} {:>+6.1} {:>6.1} {:>7.1} {:>8.1}",
            p.partial,
            p.expected_hz,
            p.freq_hz,
            1200.0 * (p.freq_hz / p.expected_hz).log2(),
            p.db,
            p.left_db,
            p.right_db
        );
    }
    s
}

const LEFT_RGB: [u8; 3] = [200, 50, 40];
const RIGHT_RGB: [u8; 3] = [40, 90, 200];

/// Top panel: impulse response, a min/max stroke per column, scaled to its peak, zero line in the
/// middle, time 0 to the length at the left and right edges. Bottom panel: magnitude response on a
/// log frequency axis from 20 Hz to Nyquist, the top of the scale at the next multiple of 10 dB
/// above the strongest bin and 90 dB of range below it, vertical lines at 100 Hz, 1 kHz and
/// 10 kHz, horizontal lines every 20 dB. Left is red, right is blue.
pub(crate) fn plot_png(a: &Analysis) -> Result<Vec<u8>, String> {
    let h = 2 * PANEL_H;
    let mut c = Canvas::new(h);
    let inner = PANEL_H - 2 * MARGIN;

    // Impulse panel.
    c.hline(MARGIN + inner / 2, AXIS);
    let peak = a
        .left
        .iter()
        .chain(&a.right)
        .fold(0.0f32, |m, v| m.max(v.abs()))
        .max(1e-12);
    let y_of = |v: f32| MARGIN + ((1.0 - v / peak) * 0.5 * (inner - 1) as f32).round() as usize;
    for (samples, rgb) in [(&a.left, LEFT_RGB), (&a.right, RIGHT_RGB)] {
        for x in 0..PLOT_W {
            let lo = x * samples.len() / PLOT_W;
            let hi = ((x + 1) * samples.len() / PLOT_W)
                .max(lo + 1)
                .min(samples.len());
            let (mn, mx) = samples[lo..hi]
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &s| (a.min(s), b.max(s)));
            c.vline(x, y_of(mx), y_of(mn), rgb);
        }
    }

    // Response panel.
    let base = PANEL_H;
    let nyq = f64::from(a.sample_rate) / 2.0;
    let top_db = (a
        .left_db
        .iter()
        .chain(&a.right_db)
        .skip((MIN_HZ / a.bin_hz).ceil() as usize)
        .cloned()
        .fold(f64::MIN, f64::max)
        / 10.0)
        .ceil()
        * 10.0;
    let x_of_hz = |f: f64| ((f / MIN_HZ).ln() / (nyq / MIN_HZ).ln() * (PLOT_W - 1) as f64).round();
    let y_of_db = |d: f64| {
        let t = ((top_db - d) / PLOT_DB_RANGE).clamp(0.0, 1.0);
        base + MARGIN + (t * (inner - 1) as f64).round() as usize
    };
    for step in 0..=(PLOT_DB_RANGE as usize / 20) {
        c.hline(y_of_db(top_db - 20.0 * step as f64), GRID);
    }
    for f in [100.0, 1_000.0, 10_000.0] {
        if f < nyq {
            let x = x_of_hz(f) as usize;
            c.vline(x, base + MARGIN, base + MARGIN + inner - 1, GRID);
        }
    }
    for (mags, rgb) in [(&a.left_db, LEFT_RGB), (&a.right_db, RIGHT_RGB)] {
        let mut prev: Option<(usize, usize)> = None;
        for x in 0..PLOT_W {
            let f0 = MIN_HZ * (nyq / MIN_HZ).powf(x as f64 / PLOT_W as f64);
            let f1 = MIN_HZ * (nyq / MIN_HZ).powf((x + 1) as f64 / PLOT_W as f64);
            let k0 = (f0 / a.bin_hz).ceil() as usize;
            let k1 = ((f1 / a.bin_hz).ceil() as usize).min(mags.len());
            let d = if k0 < k1 {
                mags[k0..k1].iter().cloned().fold(f64::MIN, f64::max)
            } else {
                mags[((f0 / a.bin_hz).round() as usize).min(mags.len() - 1)]
            };
            let y = y_of_db(d);
            // Join to the previous column so a narrow peak stays one connected stroke.
            let from = prev.map_or(y, |(_, py)| py);
            c.vline(x, from, y, rgb);
            prev = Some((x, y));
        }
    }

    c.encode_png()
}

/// Run the command: write the three files and return the peaks report.
pub fn run_analyze(args: &AnalyzeArgs) -> Result<String, String> {
    let controls = parse_resonator_controls(&args.params)?;
    let a = analyze(args.sample_rate as f32, &controls.params(), args.length_s)?;
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
    write("_impulse.csv", impulse_csv(&a).as_bytes())?;
    write("_response.csv", response_csv(&a).as_bytes())?;
    write(".png", &plot_png(&a)?)?;
    Ok(peaks_report(&a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(params: &[&str]) -> Analysis {
        let raw: Vec<String> = params.iter().map(|s| s.to_string()).collect();
        let controls = parse_resonator_controls(&raw).unwrap();
        analyze(48_000.0, &controls.params(), None).unwrap()
    }

    fn peak_hz(a: &Analysis, partial: usize) -> f64 {
        a.peaks
            .iter()
            .find(|p| p.partial == partial)
            .unwrap_or_else(|| panic!("no peak for partial {partial}: {}", peaks_report(a)))
            .freq_hz
    }

    #[test]
    fn default_voicing_peaks_sit_at_the_tuned_pitches() {
        let a = run(&[]);
        for n in 1..=VOICE_COUNT {
            let want = 110.0 * n as f64;
            let got = peak_hz(&a, n);
            assert!(
                (got - want).abs() / want < 0.01,
                "partial {n}: {got} Hz vs {want} Hz\n{}",
                peaks_report(&a)
            );
        }
        assert_eq!(a.peaks.len(), VOICE_COUNT, "{}", peaks_report(&a));
    }

    #[test]
    fn structure_stretches_the_upper_partials() {
        let a = run(&["structure=1"]);
        let p8 = peak_hz(&a, 8);
        let want = 110.0 * 8.0 * (1.0 + 0.02 * 63.0f64).sqrt();
        assert!((p8 - want).abs() / want < 0.01, "{p8} vs {want}");
        assert!(p8 > 1.3 * 880.0, "partial 8 should sit near 12x root: {p8}");
        assert!((peak_hz(&a, 1) - 110.0).abs() < 1.0, "root does not move");
    }

    #[test]
    fn damping_shortens_the_ring_and_darkens_the_top() {
        let level = |a: &Analysis, n: usize| a.peaks.iter().find(|p| p.partial == n).unwrap().db;
        let with_length = |damping: &str| {
            let raw = vec![damping.to_string()];
            let controls = parse_resonator_controls(&raw).unwrap();
            analyze(48_000.0, &controls.params(), Some(1.0)).unwrap()
        };
        let lo = with_length("damping=0.2");
        let hi = with_length("damping=0.8");
        // Energy left after 0.5 s: the more damped bank has rung down.
        let late = |a: &Analysis| -> f32 { a.left[24_000..].iter().map(|v| v * v).sum::<f32>() };
        assert!(late(&hi) < 0.1 * late(&lo));
        // Higher partials are damped harder relative to the root.
        assert!(level(&hi, 1) - level(&hi, 8) > level(&lo, 1) - level(&lo, 8));
    }

    #[test]
    fn spread_controls_the_left_right_difference() {
        let mono = run(&["spread=0"]);
        assert_eq!(mono.left, mono.right);
        assert_eq!(mono.left_db, mono.right_db);
        let wide = run(&["spread=1"]);
        let diff = wide
            .left_db
            .iter()
            .zip(&wide.right_db)
            .map(|(l, r)| (l - r).abs())
            .fold(0.0, f64::max);
        assert!(
            diff > 3.0,
            "spread 1 should differ between channels: {diff}"
        );
    }

    #[test]
    fn output_is_byte_identical_between_runs() {
        let dir = tempfile::tempdir().unwrap();
        let go = |name: &str| {
            let prefix = dir.path().join(name).to_str().unwrap().to_string();
            let args = AnalyzeArgs {
                params: vec!["root_hz=220".into(), "structure=0.5".into()],
                sample_rate: 48_000,
                length_s: Some(0.5),
                output_prefix: prefix.clone(),
            };
            let report = run_analyze(&args).unwrap();
            let read = |s: &str| std::fs::read(format!("{prefix}{s}")).unwrap();
            (
                report,
                read("_impulse.csv"),
                read("_response.csv"),
                read(".png"),
            )
        };
        let a = go("a");
        let b = go("b");
        assert_eq!(a, b);
        assert!(a.3.starts_with(b"\x89PNG"));
    }

    #[test]
    fn bad_parameters_are_refused_naming_them() {
        let go = |params: &[&str], length_s| {
            run_analyze(&AnalyzeArgs {
                params: params.iter().map(|s| s.to_string()).collect(),
                sample_rate: 48_000,
                length_s,
                output_prefix: "/nonexistent/never".into(),
            })
            .unwrap_err()
        };
        assert!(go(&["bogus=1"], None).contains("bogus"));
        assert!(go(&["root_hz=1000000"], None).contains("root_hz"));
        assert!(go(&["damping=-5"], None).contains("damping"));
        assert!(go(&["damping"], None).contains("key=value"));
        assert!(go(&[], Some(100.0)).contains("length_s"));
        let err = analyze(100.0, &Default::default(), None).unwrap_err();
        assert!(err.contains("sample_rate"));
    }
}
