//! `review`: render a manifest of cases and write one self-contained HTML page for listening
//! away from the bench. The manifest format and the pass/fail rules are in
//! `docs/reference/review.md`.
//!
//! Each case is rendered through [`crate::process::run_process`], so the page hears exactly what
//! `process` writes. The only analysis reused is `asperitas_measure::param_step`; peak level and
//! clipping are computed here because `asperitas-measure` has no check for an offline f32 render
//! (its discontinuity metrics need a loopback capture and its alignment).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use asperitas_measure::param_step::detect_param_steps;
use base64::Engine as _;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::Deserialize;

use crate::analyze::{analyze, peaks_report, plot_png};
use crate::automation::Automation;
use crate::process::{parse_resonator_controls, run_process, ProcessArgs};
use crate::wav_io;
use asperitas_dsp::asperitas::BLOCK_FRAMES;
use asperitas_dsp::resonator_controls::ResonatorControls;

/// Sample rate every clip must have; the effect and the device run at this rate.
const REQUIRED_RATE: u32 = 48_000;
/// A sample at or above this magnitude counts as clipped.
const CLIP_LEVEL: f32 = 0.999;
/// Page size limit: the artifact page limit is 16 MB, this leaves room for the markup.
const MAX_PAGE_BYTES: usize = 14 * 1024 * 1024;
const FFT_SIZE: usize = 1024;
const SPECTROGRAM_ROWS: usize = 256;
const MAX_COLUMNS: usize = 1000;

/// Arguments for the review subcommand.
pub struct ReviewArgs {
    pub manifest_path: String,
    pub output_path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    case: Vec<Case>,
    /// `"float32"` (default) embeds each render exactly as `process` writes it; `"pcm16"`
    /// embeds it as 16-bit PCM to halve its size. The checks always run on the float render.
    output_format: Option<String>,
    #[serde(default)]
    response: Vec<Response>,
}

/// An impulse and frequency response of the resonator at one setting, as `analyze` draws it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    name: String,
    /// `key=value` resonator settings, as `process --params`.
    #[serde(default)]
    params: Vec<String>,
    /// Impulse length in seconds; the root T60 when absent.
    length_s: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    clip: String,
    /// `"asperitas"` (default) or `"resonator"`, as `process --processor`.
    processor: Option<String>,
    /// Automation file; mutually exclusive with `params`.
    automation: Option<String>,
    /// `key=value` knob settings, as `process --params`.
    #[serde(default)]
    params: Vec<String>,
    /// `"dry"` (the input clip) or the path of a WAV, such as a previous render.
    reference: Option<String>,
}

struct Check {
    name: &'static str,
    value: String,
    pass: bool,
}

/// Run the review command: build the page and write it to `output_path`.
pub fn run_review(args: &ReviewArgs) -> Result<(), String> {
    let page = build_page(Path::new(&args.manifest_path))?;
    std::fs::write(&args.output_path, &page)
        .map_err(|e| format!("cannot write '{}': {e}", args.output_path))?;
    println!("wrote {} ({} bytes)", args.output_path, page.len());
    Ok(())
}

/// Render every case in the manifest and return the page. Paths in the manifest resolve
/// relative to the manifest's directory.
pub fn build_page(manifest_path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read manifest '{}': {e}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&text)
        .map_err(|e| format!("bad manifest '{}': {e}", manifest_path.display()))?;
    if manifest.case.is_empty() {
        return Err(format!(
            "manifest '{}' has no [[case]] entries",
            manifest_path.display()
        ));
    }
    let base = manifest_path.parent().unwrap_or(Path::new("."));
    for (i, case) in manifest.case.iter().enumerate() {
        if case.name.trim().is_empty() {
            return Err(format!("case {}: name is empty", i + 1));
        }
        if manifest.case[..i].iter().any(|c| c.name == case.name) {
            return Err(format!("case '{}': name appears twice", case.name));
        }
    }

    let pcm16 = match manifest.output_format.as_deref() {
        None | Some("float32") => false,
        Some("pcm16") => true,
        Some(other) => {
            return Err(format!(
                "output_format '{other}' is not one of \"float32\", \"pcm16\""
            ))
        }
    };
    for (i, r) in manifest.response.iter().enumerate() {
        if r.name.trim().is_empty() {
            return Err(format!("response {}: name is empty", i + 1));
        }
        if manifest.response[..i].iter().any(|o| o.name == r.name) {
            return Err(format!("response '{}': name appears twice", r.name));
        }
    }

    let tmp = tempfile::tempdir().map_err(|e| format!("cannot create temp dir: {e}"))?;
    let mut rendered = Vec::new();
    for (i, case) in manifest.case.iter().enumerate() {
        rendered.push(render_case(case, base, tmp.path(), i, pcm16)?);
    }

    let mut responses = Vec::new();
    for r in &manifest.response {
        responses.push(render_response(r)?);
    }

    let page = page_html(&rendered, &responses);
    if page.len() > MAX_PAGE_BYTES {
        return Err(format!(
            "page is {} bytes, over the {MAX_PAGE_BYTES} byte limit for an artifact page; \
             use shorter clips, fewer cases or output_format = \"pcm16\"",
            page.len()
        ));
    }
    Ok(page)
}

/// A response as the page shows it.
struct RenderedResponse {
    name: String,
    params: String,
    png: Vec<u8>,
    peaks: String,
}

fn render_response(r: &Response) -> Result<RenderedResponse, String> {
    let name = &r.name;
    let controls =
        parse_resonator_controls(&r.params).map_err(|e| format!("response '{name}': {e}"))?;
    let a = analyze(REQUIRED_RATE as f32, &controls.params(), r.length_s)
        .map_err(|e| format!("response '{name}': {e}"))?;
    Ok(RenderedResponse {
        name: name.clone(),
        params: if r.params.is_empty() {
            "defaults (every knob at its default)".to_string()
        } else {
            r.params.join(" ")
        },
        png: plot_png(&a).map_err(|e| format!("response '{name}': {e}"))?,
        peaks: peaks_report(&a),
    })
}

struct Rendered {
    name: String,
    settings: String,
    output: Media,
    reference: Option<(String, Media)>,
    checks: Vec<Check>,
}

impl Rendered {
    fn failed(&self) -> bool {
        self.checks.iter().any(|c| !c.pass)
    }
}

/// A clip as the page shows it: the embedded file, and its two plots.
struct Media {
    wav: Vec<u8>,
    waveform: String,
    spectrogram_png: Vec<u8>,
    seconds: f64,
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    base.join(path)
}

fn path_str(p: &Path) -> Result<&str, String> {
    p.to_str()
        .ok_or_else(|| format!("path '{}' is not valid UTF-8", p.display()))
}

/// Read a 48 kHz clip, naming the case and field on failure.
fn read_clip(case: &str, field: &str, path: &Path) -> Result<Vec<f32>, String> {
    let p = path_str(path)?;
    if !path.is_file() {
        return Err(format!(
            "case '{case}': {field} '{}' does not exist",
            path.display()
        ));
    }
    let (spec, frames) = wav_io::read_wav(p).map_err(|e| format!("case '{case}': {field}: {e}"))?;
    if spec.sample_rate != REQUIRED_RATE {
        return Err(format!(
            "case '{case}': {field} '{}' is {} Hz, expected {REQUIRED_RATE} Hz",
            path.display(),
            spec.sample_rate
        ));
    }
    Ok(frames.iter().map(|f| f[0]).collect())
}

fn render_case(
    case: &Case,
    base: &Path,
    tmp: &Path,
    index: usize,
    pcm16: bool,
) -> Result<Rendered, String> {
    let name = &case.name;
    let processor = case.processor.as_deref().unwrap_or("asperitas");
    if !matches!(processor, "asperitas" | "resonator") {
        return Err(format!(
            "case '{name}': processor '{processor}' is not one of \"asperitas\", \"resonator\""
        ));
    }
    let clip_path = resolve(base, &case.clip);
    let dry = read_clip(name, "clip", &clip_path)?;

    let automation_text = match &case.automation {
        Some(a) => {
            if !case.params.is_empty() {
                return Err(format!(
                    "case '{name}': automation and params both set knobs; use one"
                ));
            }
            let p = resolve(base, a);
            if !p.is_file() {
                return Err(format!(
                    "case '{name}': automation '{}' does not exist",
                    p.display()
                ));
            }
            Some(
                std::fs::read_to_string(&p)
                    .map_err(|e| format!("case '{name}': cannot read automation: {e}"))?,
            )
        }
        None => None,
    };

    let reference = match case.reference.as_deref() {
        None => None,
        Some("dry") => Some(("dry (input clip)".to_string(), clip_path.clone())),
        Some(p) => {
            let path = resolve(base, p);
            read_clip(name, "reference", &path)?;
            Some((format!("reference {p}"), path))
        }
    };

    let out_path = tmp.join(format!("case-{index}.wav"));
    run_process(&ProcessArgs {
        input_path: path_str(&clip_path)?.to_string(),
        output_path: path_str(&out_path)?.to_string(),
        processor_name: processor.to_string(),
        params: case.params.clone(),
        automation: case
            .automation
            .as_ref()
            .map(|a| resolve(base, a).to_string_lossy().into_owned()),
    })
    .map_err(|e| format!("case '{name}': {e}"))?;

    let out_path_s = path_str(&out_path)?;
    let (_, out_frames) = wav_io::read_wav(out_path_s)?;
    let (left, right): (Vec<f32>, Vec<f32>) = out_frames.iter().map(|f| (f[0], f[1])).unzip();
    let wav = if pcm16 {
        pcm16_wav(&left, &right)
    } else {
        std::fs::read(&out_path).map_err(|e| format!("case '{name}': {e}"))?
    };
    let output = media(wav, &left)?;

    let step_frames = match &automation_text {
        Some(text) if processor == "resonator" => {
            Automation::<ResonatorControls>::parse_for(text, REQUIRED_RATE)
                .map_err(|e| format!("case '{name}': {e}"))?
                .step_frames()
        }
        Some(text) => Automation::parse(text, REQUIRED_RATE)
            .map_err(|e| format!("case '{name}': {e}"))?
            .step_frames(),
        None => Vec::new(),
    };
    let checks = run_checks(&left, &right, &dry, &step_frames);

    let reference = match reference {
        None => None,
        Some((label, path)) => {
            let samples = read_clip(name, "reference", &path)?;
            let bytes = std::fs::read(&path).map_err(|e| format!("case '{name}': {e}"))?;
            Some((label, media(bytes, &samples)?))
        }
    };

    let settings = match (&automation_text, case.params.is_empty()) {
        (Some(text), _) => format!(
            "automation {}:\n{}",
            case.automation.as_deref().unwrap_or(""),
            text
        ),
        (None, true) => "defaults (every knob at its default)".to_string(),
        (None, false) => format!("params: {}", case.params.join(" ")),
    };

    Ok(Rendered {
        name: name.clone(),
        settings,
        output,
        reference,
        checks,
    })
}

/// Detector events at scripted steps are expected, so they are not counted against the render. The
/// detector measures level only, so an event is excused by its time alone: it cannot say which
/// parameter moved.
fn run_checks(left: &[f32], right: &[f32], dry: &[f32], step_frames: &[u64]) -> Vec<Check> {
    let peak = left.iter().chain(right).fold(0.0f32, |p, s| p.max(s.abs()));
    let clipped = left
        .iter()
        .chain(right)
        .filter(|s| s.abs() >= CLIP_LEVEL)
        .count();
    let peak_db = if peak > 0.0 {
        format!("{:.1} dBFS", 20.0 * f64::from(peak).log10())
    } else {
        "silence".to_string()
    };

    let steps = detect_param_steps(left);
    let dry_steps = detect_param_steps(dry).count;
    // Attacks in the material flag the detector too, so a step the input already has is not the
    // render's doing. A step lands at the next block start, and the detector reports a few of its
    // blocks later (or one block early, from the lag-2 comparison), hence the window. Positions
    // are capped, so the excused count is matched events and the rest of `count` stays unexpected.
    let block = asperitas_measure::thresholds::PARAM_STEP_BLOCK_SAMPLES as u64;
    let early = 2 * block;
    let late = BLOCK_FRAMES as u64
        + (asperitas_measure::thresholds::PARAM_STEP_MERGE_BLOCKS as u64 + 1) * block;
    let at_step = steps
        .positions
        .iter()
        .filter(|&&p| {
            let p = p as u64;
            step_frames.iter().any(|&f| p + early >= f && p <= f + late)
        })
        .count();
    let unexpected = steps.count - at_step;
    let steps_pass = unexpected <= dry_steps;
    let positions: Vec<String> = steps
        .positions
        .iter()
        .take(8)
        .map(|p| format!("{:.3} s", *p as f64 / f64::from(REQUIRED_RATE)))
        .collect();
    let steps_value = format!(
        "{} event(s) ({at_step} at scripted steps), largest {:.1} dB; input clip has {dry_steps}{}",
        steps.count,
        steps.max_step_db,
        if positions.is_empty() {
            String::new()
        } else {
            format!("; at {}", positions.join(", "))
        }
    );

    vec![
        Check {
            name: "peak level",
            value: peak_db,
            pass: true,
        },
        Check {
            name: "clipping",
            value: format!("{clipped} sample(s) at or above {CLIP_LEVEL} full scale"),
            pass: clipped == 0,
        },
        Check {
            name: "parameter steps",
            value: steps_value,
            pass: steps_pass,
        },
    ]
}

/// A stereo 16-bit PCM WAV at the review rate, samples rounded and clamped.
fn pcm16_wav(left: &[f32], right: &[f32]) -> Vec<u8> {
    let data_len = (left.len() * 4) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&REQUIRED_RATE.to_le_bytes());
    w.extend_from_slice(&(REQUIRED_RATE * 4).to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for (l, r) in left.iter().zip(right) {
        for s in [l, r] {
            let q = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            w.extend_from_slice(&q.to_le_bytes());
        }
    }
    w
}

fn media(wav: Vec<u8>, mono: &[f32]) -> Result<Media, String> {
    Ok(Media {
        wav,
        waveform: waveform_svg(mono),
        spectrogram_png: spectrogram_png(mono)?,
        seconds: mono.len() as f64 / f64::from(REQUIRED_RATE),
    })
}

/// Min/max envelope, one vertical stroke per pixel column.
fn waveform_svg(samples: &[f32]) -> String {
    const W: usize = MAX_COLUMNS;
    const H: f32 = 120.0;
    let mut d = String::new();
    if !samples.is_empty() {
        let cols = W.min(samples.len());
        for c in 0..cols {
            let lo = c * samples.len() / cols;
            let hi = ((c + 1) * samples.len() / cols).max(lo + 1);
            let (mn, mx) = samples[lo..hi]
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &s| (a.min(s), b.max(s)));
            let y = |v: f32| H / 2.0 - v.clamp(-1.0, 1.0) * (H / 2.0 - 1.0);
            let x = c as f32 * W as f32 / cols as f32;
            let _ = write!(d, "M{x:.1} {:.1}V{:.1}", y(mx), y(mn).max(y(mx) + 0.5));
        }
    }
    format!(
        "<svg viewBox=\"0 0 {W} {H}\" preserveAspectRatio=\"none\" role=\"img\" \
         aria-label=\"waveform\"><line x1=\"0\" x2=\"{W}\" y1=\"{m}\" y2=\"{m}\" class=\"mid\"/>\
         <path d=\"{d}\" class=\"wave\"/></svg>",
        H = H as u32,
        m = H / 2.0
    )
}

/// Hann-windowed log-magnitude spectrogram, 0 dBFS at the top of the scale and -100 dB at the
/// bottom, low frequencies at the bottom of the image.
fn spectrogram_png(samples: &[f32]) -> Result<Vec<u8>, String> {
    let hop = (samples.len() / MAX_COLUMNS).max(256);
    let cols = (samples.len().saturating_sub(FFT_SIZE) / hop + 1).max(1);
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / FFT_SIZE as f32).cos())
        .collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
    // A full-scale sine reads N/4 in a Hann-windowed bin.
    let norm = FFT_SIZE as f32 / 4.0;
    let pool = (FFT_SIZE / 2) / SPECTROGRAM_ROWS;

    let mut pixels = vec![0u8; cols * SPECTROGRAM_ROWS * 3];
    let mut buf = vec![Complex::new(0.0f32, 0.0); FFT_SIZE];
    for c in 0..cols {
        let start = c * hop;
        for (n, slot) in buf.iter_mut().enumerate() {
            let s = samples.get(start + n).copied().unwrap_or(0.0);
            *slot = Complex::new(s * window[n], 0.0);
        }
        fft.process(&mut buf);
        for row in 0..SPECTROGRAM_ROWS {
            let mag = buf[row * pool..(row + 1) * pool]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.norm()))
                / norm;
            let db = 20.0 * (mag.max(1e-9)).log10();
            let t = ((db + 100.0) / 100.0).clamp(0.0, 1.0);
            let y = SPECTROGRAM_ROWS - 1 - row;
            let px = (y * cols + c) * 3;
            pixels[px] = (255.0 * t.sqrt()) as u8;
            pixels[px + 1] = (255.0 * t * t) as u8;
            pixels[px + 2] = (255.0 * (1.0 - t).min(t * 2.0)) as u8;
        }
    }

    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, cols as u32, SPECTROGRAM_ROWS as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| format!("png header: {e}"))?;
    w.write_image_data(&pixels)
        .map_err(|e| format!("png data: {e}"))?;
    w.finish().map_err(|e| format!("png finish: {e}"))?;
    Ok(out)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn media_html(label: &str, m: &Media) -> String {
    format!(
        "<div class=\"clip\"><h3>{}</h3>\
         <audio controls preload=\"metadata\" src=\"data:audio/wav;base64,{}\"></audio>\
         <div class=\"plot\">{}</div>\
         <img class=\"spec\" alt=\"spectrogram\" src=\"data:image/png;base64,{}\">\
         <p class=\"note\">{:.2} s. Waveform: min/max per column. Spectrogram: 0 Hz at the \
         bottom to 24 kHz at the top, 0 dBFS (bright) to -100 dBFS (black).</p></div>",
        escape(label),
        b64(&m.wav),
        m.waveform,
        b64(&m.spectrogram_png),
        m.seconds
    )
}

fn page_html(cases: &[Rendered], responses: &[RenderedResponse]) -> String {
    let mut h = String::new();
    h.push_str(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Asperitas Review</title><style>\
         :root{--bg:#fff;--fg:#1b1b1f;--mute:#62626b;--line:#d9d9e0;--card:#f6f6f9;\
         --ok:#1a7f37;--bad:#c62828;--wave:#2457c5}\
         @media (prefers-color-scheme:dark){:root:not([data-theme=\"light\"]){--bg:#15151a;\
         --fg:#e8e8ee;--mute:#9a9aa6;--line:#33333c;--card:#1d1d24;--ok:#4cc26b;--bad:#ff6b6b;\
         --wave:#7aa2ff}}\
         :root[data-theme=\"dark\"]{--bg:#15151a;--fg:#e8e8ee;--mute:#9a9aa6;--line:#33333c;\
         --card:#1d1d24;--ok:#4cc26b;--bad:#ff6b6b;--wave:#7aa2ff}\
         body{margin:0;padding:0 16px 48px;background:var(--bg);color:var(--fg);\
         font:15px/1.5 system-ui,sans-serif}\
         main{max-width:1000px;margin:0 auto}\
         h1{font-size:1.5rem;margin:24px 0 8px}h2{font-size:1.2rem;margin:0 0 8px}\
         h3{font-size:.95rem;margin:12px 0 4px;color:var(--mute)}\
         table{border-collapse:collapse;width:100%}\
         td,th{border-bottom:1px solid var(--line);padding:6px 8px;text-align:left;\
         vertical-align:top}\
         section{background:var(--card);border:1px solid var(--line);border-radius:8px;\
         padding:12px 16px;margin:16px 0}\
         .ok{color:var(--ok);font-weight:600}.bad{color:var(--bad);font-weight:700}\
         .plot svg{width:100%;height:120px;display:block;border:1px solid var(--line)}\
         .wave{stroke:var(--wave);stroke-width:1;fill:none}\
         .mid{stroke:var(--line);stroke-width:1}\
         .spec{width:100%;height:160px;display:block;border:1px solid var(--line);\
         image-rendering:auto}\
         a{color:var(--wave)}audio{width:100%}.note{color:var(--mute);font-size:.8rem;margin:4px 0}\
         .resp{width:100%;height:auto;display:block;border:1px solid var(--line)}\
         pre{white-space:pre-wrap;overflow-wrap:anywhere;margin:4px 0}\
         </style></head><body><main><h1>Asperitas review</h1>",
    );
    h.push_str("<table><tr><th>Case</th><th>Result</th></tr>");
    for (i, c) in cases.iter().enumerate() {
        let _ = write!(
            h,
            "<tr><td><a href=\"#case-{i}\">{}</a></td><td class=\"{}\">{}</td></tr>",
            escape(&c.name),
            if c.failed() { "bad" } else { "ok" },
            if c.failed() { "FAILED" } else { "passed" }
        );
    }
    h.push_str("</table>");
    if !responses.is_empty() {
        h.push_str("<section id=\"responses\"><h2>Response</h2>");
        for r in responses {
            let _ = write!(
                h,
                "<div class=\"clip\"><h3>{}</h3><pre>{}</pre>\
                 <img class=\"resp\" alt=\"impulse and frequency response\" \
                 src=\"data:image/png;base64,{}\">\
                 <p class=\"note\">Top: impulse response. Bottom: magnitude response, 20 Hz \
                 to Nyquist; left red, right blue.</p><pre>{}</pre></div>",
                escape(&r.name),
                escape(&r.params),
                b64(&r.png),
                escape(&r.peaks)
            );
        }
        h.push_str("</section>");
    }
    for (i, c) in cases.iter().enumerate() {
        let _ = write!(
            h,
            "<section id=\"case-{i}\"><h2>{} <span class=\"{}\">{}</span></h2>\
             <h3>Settings</h3><pre>{}</pre>",
            escape(&c.name),
            if c.failed() { "bad" } else { "ok" },
            if c.failed() { "FAILED" } else { "passed" },
            escape(&c.settings)
        );
        h.push_str(&media_html("Output", &c.output));
        if let Some((label, m)) = &c.reference {
            h.push_str(&media_html(label, m));
        }
        h.push_str("<h3>Checks on the output</h3><table><tr><th>Check</th><th>Result</th><th>Detail</th></tr>");
        for ck in &c.checks {
            let _ = write!(
                h,
                "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>",
                ck.name,
                if ck.pass { "ok" } else { "bad" },
                if ck.pass { "passed" } else { "FAILED" },
                escape(&ck.value)
            );
        }
        h.push_str("</table></section>");
    }
    h.push_str("</main></body></html>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{generate_pluck, to_stereo};

    fn write_clip(path: &Path, rate: u32, gain: f32) {
        let mono: Vec<f32> = generate_pluck(rate, 0.5, 220.0)
            .iter()
            .map(|s| s * gain)
            .collect();
        wav_io::write_wav(path.to_str().unwrap(), rate, &to_stereo(&mono)).unwrap();
    }

    fn page_for(dir: &Path, manifest: &str) -> Result<String, String> {
        let m = dir.join("m.toml");
        std::fs::write(&m, manifest).unwrap();
        build_page(&m)
    }

    #[test]
    fn page_lists_cases_audio_and_checks() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 1.0);
        let page = page_for(
            dir.path(),
            "[[case]]\nname = \"plain\"\nclip = \"a.wav\"\nreference = \"dry\"\n\
             [[case]]\nname = \"quiet\"\nclip = \"a.wav\"\nparams = [\"gain_db=-6\"]\n",
        )
        .unwrap();
        for needle in [
            "plain",
            "quiet",
            "peak level",
            "clipping",
            "parameter steps",
        ] {
            assert!(page.contains(needle), "missing {needle}");
        }
        assert!(page.contains("data:audio/wav;base64,"));
        assert!(page.contains("data:image/png;base64,"));
        assert!(page.contains("dry (input clip)"));
        assert!(page.contains("passed"));
        assert!(!page.contains("FAILED"), "no case should fail");
        assert!(!page.contains("http://") && !page.contains("https://"));
    }

    #[test]
    fn failing_check_is_marked_failed() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("loud.wav"), 48_000, 1.0);
        let page = page_for(
            dir.path(),
            "[[case]]\nname = \"hot\"\nclip = \"loud.wav\"\nparams = [\"gain_db=12\"]\n",
        )
        .unwrap();
        println!(
            "{}",
            page.split("parameter steps")
                .nth(1)
                .unwrap()
                .chars()
                .take(300)
                .collect::<String>()
        );
        assert!(page.contains("FAILED"));
        assert!(page.contains("<td class=\"bad\">FAILED</td>"));
        assert!(page.contains("sample(s) at or above"));
    }

    #[test]
    fn bad_inputs_name_the_problem() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("ok.wav"), 48_000, 0.5);
        write_clip(&dir.path().join("cd.wav"), 44_100, 0.5);

        let bad = page_for(dir.path(), "not toml [[").unwrap_err();
        assert!(bad.contains("bad manifest"), "{bad}");
        let unknown = page_for(
            dir.path(),
            "[[case]]\nname=\"x\"\nclip=\"ok.wav\"\nbogus=1\n",
        )
        .unwrap_err();
        assert!(unknown.contains("bogus"), "{unknown}");
        let missing =
            page_for(dir.path(), "[[case]]\nname=\"x\"\nclip=\"nope.wav\"\n").unwrap_err();
        assert!(
            missing.contains("'x'") && missing.contains("does not exist"),
            "{missing}"
        );
        let rate = page_for(dir.path(), "[[case]]\nname=\"x\"\nclip=\"cd.wav\"\n").unwrap_err();
        assert!(
            rate.contains("44100 Hz") && rate.contains("48000"),
            "{rate}"
        );
        let dup = page_for(
            dir.path(),
            "[[case]]\nname=\"x\"\nclip=\"ok.wav\"\n[[case]]\nname=\"x\"\nclip=\"ok.wav\"\n",
        )
        .unwrap_err();
        assert!(dup.contains("twice"), "{dup}");
        let empty = page_for(dir.path(), "").unwrap_err();
        assert!(empty.contains("no [[case]]"), "{empty}");
    }

    #[test]
    fn automation_text_is_shown_and_step_lines_are_expected() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        std::fs::write(
            dir.path().join("k.txt"),
            "0 gain_db=0\n0.25 step gain_db=-12\n",
        )
        .unwrap();
        let page = page_for(
            dir.path(),
            "[[case]]\nname=\"auto\"\nclip=\"a.wav\"\nautomation=\"k.txt\"\n",
        )
        .unwrap();
        assert!(page.contains("0.25 step gain_db=-12"));
        assert!(!page.contains("FAILED"));
    }

    /// A 440 Hz tone that drops 12 dB at each frame in `drops`.
    fn tone_with_drops(drops: &[usize]) -> Vec<f32> {
        (0..48_000usize)
            .map(|i| {
                let level = 0.5 * 0.25f32.powi(drops.iter().filter(|&&d| i >= d).count() as i32);
                level * (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin()
            })
            .collect()
    }

    fn steps_check(drops: &[usize], step_frames: &[u64]) -> Check {
        let out = tone_with_drops(drops);
        let dry = tone_with_drops(&[]);
        run_checks(&out, &out, &dry, step_frames)
            .into_iter()
            .find(|c| c.name == "parameter steps")
            .unwrap()
    }

    #[test]
    fn a_scripted_step_is_expected_and_an_unscripted_one_is_not() {
        let scripted = steps_check(&[12_000], &[12_000]);
        assert!(scripted.pass, "{}", scripted.value);
        assert!(scripted.value.contains("1 at scripted steps"));
        assert!(!steps_check(&[12_000], &[]).pass);
    }

    #[test]
    fn a_step_line_does_not_excuse_a_jump_elsewhere() {
        let c = steps_check(&[12_000, 30_000], &[12_000]);
        assert!(!c.pass, "{}", c.value);
    }

    #[test]
    fn bare_default_step_lines_are_expected() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        std::fs::write(
            dir.path().join("k.txt"),
            "0 cutoff_hz=1000\n0.25 gain_db=-12\n",
        )
        .unwrap();
        let page = page_for(
            dir.path(),
            "[[case]]\nname=\"auto\"\nclip=\"a.wav\"\nautomation=\"k.txt\"\n",
        )
        .unwrap();
        assert!(!page.contains("FAILED"));
        assert!(page.contains("at scripted steps"));
    }

    #[test]
    fn resonator_cases_render_from_params_and_automation() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        std::fs::write(
            dir.path().join("k.txt"),
            "0 root_hz=220 damping=0\n0.4 ramp damping=1\n",
        )
        .unwrap();
        let page = page_for(
            dir.path(),
            "[[case]]\nname=\"p\"\nclip=\"a.wav\"\nprocessor=\"resonator\"\n\
             params=[\"root_hz=220\"]\n\
             [[case]]\nname=\"a\"\nclip=\"a.wav\"\nprocessor=\"resonator\"\n\
             automation=\"k.txt\"\n",
        )
        .unwrap();
        assert!(page.contains("params: root_hz=220"));
        assert!(page.contains("0.4 ramp damping=1"));
    }

    #[test]
    fn bad_processor_and_resonator_key_name_the_case() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        let bad_key = page_for(
            dir.path(),
            "[[case]]\nname=\"x\"\nclip=\"a.wav\"\nprocessor=\"resonator\"\nparams=[\"cutoff_hz=1\"]\n",
        )
        .unwrap_err();
        assert!(
            bad_key.contains("'x'") && bad_key.contains("cutoff_hz"),
            "{bad_key}"
        );
        let bad_proc = page_for(
            dir.path(),
            "[[case]]\nname=\"x\"\nclip=\"a.wav\"\nprocessor=\"gain\"\n",
        )
        .unwrap_err();
        assert!(
            bad_proc.contains("'x'") && bad_proc.contains("processor"),
            "{bad_proc}"
        );
    }

    #[test]
    fn response_section_embeds_the_plot_and_peaks() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        let page = page_for(
            dir.path(),
            "[[case]]\nname=\"c\"\nclip=\"a.wav\"\n\
             [[response]]\nname=\"damped\"\nparams=[\"damping=1\"]\n",
        )
        .unwrap();
        assert!(page.contains("<h2>Response</h2>"));
        assert!(page.contains("damped") && page.contains("damping=1"));
        assert!(page.contains("class=\"resp\""));
        assert!(page.contains("partial expected_hz peak_hz"));

        let bad = page_for(
            dir.path(),
            "[[case]]\nname=\"c\"\nclip=\"a.wav\"\n\
             [[response]]\nname=\"r\"\nparams=[\"bogus=1\"]\n",
        )
        .unwrap_err();
        assert!(
            bad.contains("response 'r'") && bad.contains("bogus"),
            "{bad}"
        );
        let dup = page_for(
            dir.path(),
            "[[case]]\nname=\"c\"\nclip=\"a.wav\"\n[[response]]\nname=\"r\"\n[[response]]\nname=\"r\"\n",
        )
        .unwrap_err();
        assert!(dup.contains("twice"), "{dup}");
    }

    #[test]
    fn pcm16_output_is_smaller_and_the_page_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        write_clip(&dir.path().join("a.wav"), 48_000, 0.5);
        let case = "[[case]]\nname=\"c\"\nclip=\"a.wav\"\nprocessor=\"resonator\"\n\
                    [[response]]\nname=\"r\"\n";
        let float = page_for(dir.path(), case).unwrap();
        let pcm = page_for(dir.path(), &format!("output_format=\"pcm16\"\n{case}")).unwrap();
        assert!(pcm.len() < float.len());
        assert_eq!(
            pcm,
            page_for(dir.path(), &format!("output_format=\"pcm16\"\n{case}")).unwrap()
        );
        assert_eq!(float, page_for(dir.path(), case).unwrap());
        let bad = page_for(dir.path(), &format!("output_format=\"mp3\"\n{case}")).unwrap_err();
        assert!(bad.contains("output_format"), "{bad}");
    }

    #[test]
    fn committed_resonator_manifest_parses_and_its_files_exist() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../audio/review");
        let text = std::fs::read_to_string(dir.join("resonator.toml")).unwrap();
        let m: Manifest = toml::from_str(&text).unwrap();
        assert!(m.case.len() >= 12 && !m.response.is_empty());
        for c in &m.case {
            assert!(dir.join(&c.clip).is_file(), "{}", c.clip);
            if let Some(a) = &c.automation {
                let t = std::fs::read_to_string(dir.join(a)).unwrap();
                Automation::<ResonatorControls>::parse_for(&t, REQUIRED_RATE).unwrap();
            }
        }
    }
}
