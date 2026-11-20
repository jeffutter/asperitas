//! Process a WAV file through a named DSP processor.

use asperitas_dsp::asperitas::{Asperitas, Controls};
use asperitas_dsp::filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
use asperitas_dsp::gain::{Gain, GainParams, GAIN_PARAMS};
use asperitas_dsp::param::ParamSpec;
use asperitas_dsp::processor::Processor;
use asperitas_dsp::resonator_bank::Resonator;
use asperitas_dsp::resonator_controls::ResonatorControls;

use crate::automation::Automation;
use crate::wav_io;

/// Arguments for the process subcommand.
pub struct ProcessArgs {
    pub input_path: String,
    pub output_path: String,
    pub processor_name: String,
    pub params: Vec<String>,
    /// Path of a knob automation file; only the `asperitas` and `resonator`
    /// processors have knobs to move.
    pub automation: Option<String>,
}

/// Run the process command: read input WAV, process it, write output WAV.
pub fn run_process(args: &ProcessArgs) -> Result<(), String> {
    let (spec, frames) = wav_io::read_wav(&args.input_path)?;

    let mut frames = frames;

    let sample_rate = spec.sample_rate as f32;
    let pairs = parse_param_pairs(&args.params)?;
    if args.automation.is_some()
        && !matches!(args.processor_name.as_str(), "asperitas" | "resonator")
    {
        return Err(format!(
            "--automation needs the asperitas or resonator processor; '{}' has no knobs",
            args.processor_name
        ));
    }
    match args.processor_name.as_str() {
        "gain" => {
            let v = parse_with_table("gain", &GAIN_PARAMS, &pairs)?;
            Gain::new(sample_rate, &GainParams { gain_db: v[0] }).process_block(&mut frames);
        }
        "filter" => {
            let v = parse_with_table("filter", &FILTER_PARAMS, &pairs)?;
            OnePoleLowPass::new(sample_rate, &FilterParams { cutoff_hz: v[0] })
                .process_block(&mut frames);
        }
        "asperitas" => {
            if let Some(path) = &args.automation {
                if !pairs.is_empty() {
                    return Err(
                        "--automation and --params both set knobs; put the starting values \
                         in the automation file"
                            .to_string(),
                    );
                }
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("cannot read automation file '{path}': {e}"))?;
                let automation = Automation::parse(&text, spec.sample_rate)
                    .map_err(|e| format!("{path}: {e}"))?;
                let mut effect = Asperitas::new(sample_rate, &automation.controls_at(0));
                automation.render(&mut effect, &mut frames);
            } else {
                let controls = controls_from_pairs(&pairs)?;
                Asperitas::new(sample_rate, &controls).process_block(&mut frames);
            }
        }
        "resonator" => {
            if let Some(path) = &args.automation {
                if !pairs.is_empty() {
                    return Err(
                        "--automation and --params both set knobs; put the starting values \
                         in the automation file"
                            .to_string(),
                    );
                }
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("cannot read automation file '{path}': {e}"))?;
                let automation =
                    Automation::<ResonatorControls>::parse_for(&text, spec.sample_rate)
                        .map_err(|e| format!("{path}: {e}"))?;
                let mut bank = Resonator::new(sample_rate, &automation.controls_at(0).params());
                automation.render(&mut bank, &mut frames);
            } else {
                let controls = resonator_controls_from_pairs(&pairs)?;
                Resonator::new(sample_rate, &controls.params()).process_block(&mut frames);
            }
        }
        other => {
            return Err(format!(
                "unknown processor '{other}'; supported: gain, filter, asperitas, resonator"
            ))
        }
    }

    // Output is always stereo regardless of the input's channel count; only the rate
    // carries over from the input file.
    wav_io::write_wav(&args.output_path, spec.sample_rate, &frames)?;
    Ok(())
}

/// Resolve `key=value` pairs against a processor's parameter table. Returns one value per table
/// entry, in table order, starting from each entry's default. Values are not range-checked:
/// processors sanitise their own inputs.
fn parse_with_table(
    processor: &str,
    table: &[ParamSpec],
    pairs: &[(String, String)],
) -> Result<Vec<f32>, String> {
    let mut values: Vec<f32> = table.iter().map(|spec| spec.default).collect();
    for (key, value) in pairs {
        let idx = table
            .iter()
            .position(|spec| spec.name == key)
            .ok_or_else(|| format!("unknown parameter key for {processor} processor: {key}"))?;
        values[idx] = value
            .parse::<f32>()
            .map_err(|_| format!("invalid value for {key}: {value}"))?;
    }
    Ok(values)
}

/// Translate `key=value` arguments (real units, as `--params cutoff_hz=2000 gain_db=-6`) into the
/// effect's [`Controls`]. Every surface that drives the effect from text goes through this.
pub fn parse_asperitas_controls(raw: &[String]) -> Result<Controls, String> {
    controls_from_pairs(&parse_param_pairs(raw)?)
}

fn controls_from_pairs(pairs: &[(String, String)]) -> Result<Controls, String> {
    let mut values = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let value = value
            .parse::<f32>()
            .map_err(|_| format!("invalid value for {key}: {value}"))?;
        values.push((key.as_str(), value));
    }
    Controls::from_values(&values).map_err(|e| e.to_string())
}

/// As [`parse_asperitas_controls`], for the resonator's controls (real units, as
/// `--params root_hz=220 damping=0.4`).
pub fn parse_resonator_controls(raw: &[String]) -> Result<ResonatorControls, String> {
    resonator_controls_from_pairs(&parse_param_pairs(raw)?)
}

fn resonator_controls_from_pairs(pairs: &[(String, String)]) -> Result<ResonatorControls, String> {
    let mut values = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let value = value
            .parse::<f32>()
            .map_err(|_| format!("invalid value for {key}: {value}"))?;
        values.push((key.as_str(), value));
    }
    ResonatorControls::from_values(&values).map_err(|e| e.to_string())
}

fn parse_param_pairs(raw: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for arg in raw {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(format!("parameter must be key=value, got '{arg}'"));
        }
        pairs.push((parts[0].to_string(), parts[1].to_string()));
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_dsp::asperitas::ASPERITAS_PARAMS;

    #[test]
    fn gain_parse_rejects_unknown_key() {
        let err = parse_with_table("gain", &GAIN_PARAMS, &[("bogus_key".into(), "1.0".into())])
            .unwrap_err();
        assert!(err.contains("unknown parameter"), "got: {err}");
    }

    #[test]
    fn filter_parse_rejects_non_numeric_value() {
        let err = parse_with_table(
            "filter",
            &FILTER_PARAMS,
            &[("cutoff_hz".into(), "not_a_number".into())],
        )
        .unwrap_err();
        assert!(err.contains("invalid value"), "got: {err}");
    }

    #[test]
    fn parse_uses_table_defaults_and_overrides() {
        let v = parse_with_table("filter", &FILTER_PARAMS, &[]).unwrap();
        assert_eq!(v, vec![FILTER_PARAMS[0].default]);
        let v = parse_with_table("gain", &GAIN_PARAMS, &[("gain_db".into(), "-6".into())]).unwrap();
        assert_eq!(v, vec![-6.0]);
    }

    #[test]
    fn parse_param_pairs_rejects_malformed_arg() {
        let err = parse_param_pairs(&["not_a_kv_pair".to_string()]).unwrap_err();
        assert!(
            err.contains("key=value"),
            "expected error to mention key=value format, got: {err}"
        );
    }

    /// Mono in, stereo out, with a header that agrees with the payload.
    ///
    /// Regression: the output spec was inherited from the input, so a mono file yielded
    /// a WAV claiming one channel while `write_wav` had written two — the frame count
    /// read back doubled and the audio played at half speed.
    #[test]
    fn run_process_writes_valid_stereo_from_mono_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mono_in.wav");
        let output = dir.path().join("out.wav");

        const FRAMES: usize = 64;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&input, spec).unwrap();
        for i in 0..FRAMES {
            writer.write_sample((i as i16) * 100).unwrap();
        }
        writer.finalize().unwrap();

        run_process(&ProcessArgs {
            input_path: input.to_str().unwrap().to_string(),
            output_path: output.to_str().unwrap().to_string(),
            processor_name: "gain".to_string(),
            params: vec!["gain_db=0".to_string()],
            automation: None,
        })
        .unwrap();

        let reader = hound::WavReader::open(&output).unwrap();
        let out_spec = reader.spec();
        assert_eq!(out_spec.channels, 2, "output must be stereo");
        assert_eq!(out_spec.sample_rate, 48_000, "rate carries over from input");
        assert_eq!(
            reader.len() as usize,
            FRAMES * 2,
            "header must agree with the interleaved stereo payload"
        );
    }

    fn run_on_ramp(processor: &str, params: &[&str]) -> Vec<f32> {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let output = dir.path().join("out.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&input, spec).unwrap();
        for i in 0..2000i32 {
            writer
                .write_sample(((i % 200) * 100 - 10_000) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
        run_process(&ProcessArgs {
            input_path: input.to_str().unwrap().to_string(),
            output_path: output.to_str().unwrap().to_string(),
            processor_name: processor.to_string(),
            params: params.iter().map(|p| p.to_string()).collect(),
            automation: None,
        })
        .unwrap();
        hound::WavReader::open(&output)
            .unwrap()
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect()
    }

    #[test]
    fn asperitas_rejects_unknown_key() {
        let err = process_err(&["bogus=1"]);
        assert!(err.contains("unknown parameter"), "got: {err}");
    }

    fn process_err(params: &[&str]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&input, spec).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();
        run_process(&ProcessArgs {
            input_path: input.to_str().unwrap().to_string(),
            output_path: dir.path().join("o.wav").to_str().unwrap().to_string(),
            processor_name: "asperitas".to_string(),
            params: params.iter().map(|p| p.to_string()).collect(),
            automation: None,
        })
        .unwrap_err()
    }

    #[test]
    fn asperitas_rejects_out_of_range() {
        for (arg, key) in [
            ("gain_db=20", "gain_db"),
            ("cutoff_hz=0", "cutoff_hz"),
            ("cutoff_hz=1e9", "cutoff_hz"),
            ("gain_db=nan", "gain_db"),
        ] {
            let err = process_err(&[arg]);
            assert!(
                err.contains(key) && err.contains("out of range") && err.contains("allowed"),
                "{arg}: {err}"
            );
        }
    }

    #[test]
    fn asperitas_accepts_boundary_values() {
        for spec in &ASPERITAS_PARAMS {
            for v in [spec.min, spec.max] {
                run_on_ramp("asperitas", &[&format!("{}={v}", spec.name)]);
            }
        }
    }

    #[test]
    fn asperitas_matches_filter_then_gain() {
        let chained = run_on_ramp("asperitas", &["cutoff_hz=1000", "gain_db=-6"]);
        let filtered = run_on_ramp("filter", &["cutoff_hz=1000"]);
        // Gain stage applied to the filtered file via a second pass.
        let dir = tempfile::tempdir().unwrap();
        let mid = dir.path().join("mid.wav");
        let out = dir.path().join("out.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&mid, spec).unwrap();
        for s in &filtered {
            w.write_sample(*s).unwrap();
        }
        w.finalize().unwrap();
        run_process(&ProcessArgs {
            input_path: mid.to_str().unwrap().to_string(),
            output_path: out.to_str().unwrap().to_string(),
            processor_name: "gain".to_string(),
            params: vec!["gain_db=-6".to_string()],
            automation: None,
        })
        .unwrap();
        let staged: Vec<f32> = hound::WavReader::open(&out)
            .unwrap()
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(chained.len(), staged.len());
        for (a, b) in chained.iter().zip(&staged) {
            assert!((a - b).abs() <= 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn asperitas_defaults_match_default_effect() {
        let out = run_on_ramp("asperitas", &[]);
        let unity = run_on_ramp("gain", &["gain_db=0"]);
        assert_eq!(out.len(), unity.len());
        // Default cutoff is open, default gain 0 dB: close to passthrough.
        let max_diff = out
            .iter()
            .zip(&unity)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_diff < 0.05, "{max_diff:?}");
    }

    #[test]
    fn automation_file_moves_knobs_and_is_refused_off_the_asperitas_processor() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let auto = dir.path().join("a.txt");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&input, spec).unwrap();
        for i in 0..2000i32 {
            w.write_sample(((i % 200) * 100 - 10_000) as i16).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(&auto, "0 gain_db=0\n0.01 ramp gain_db=-20\n").unwrap();
        let run = |name: &str, processor: &str, automation: Option<&std::path::Path>| {
            let out = dir.path().join(name);
            let r = run_process(&ProcessArgs {
                input_path: input.to_str().unwrap().to_string(),
                output_path: out.to_str().unwrap().to_string(),
                processor_name: processor.to_string(),
                params: vec![],
                automation: automation.map(|p| p.to_str().unwrap().to_string()),
            });
            (r, out)
        };
        let (r, moved) = run("moved.wav", "asperitas", Some(&auto));
        r.unwrap();
        let (r, plain) = run("plain.wav", "asperitas", None);
        r.unwrap();
        let read = |p: &std::path::Path| -> Vec<f32> {
            hound::WavReader::open(p)
                .unwrap()
                .samples::<f32>()
                .map(|s| s.unwrap())
                .collect()
        };
        let (moved, plain) = (read(&moved), read(&plain));
        assert_eq!(moved.len(), plain.len());
        assert_ne!(moved, plain, "automation must change the output");
        let (r, _) = run("g.wav", "gain", Some(&auto));
        assert!(r.unwrap_err().contains("--automation"));
    }

    fn write_ramp(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..2000i32 {
            w.write_sample(((i % 200) * 100 - 10_000) as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    fn read_f32(p: &std::path::Path) -> Vec<f32> {
        hound::WavReader::open(p)
            .unwrap()
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect()
    }

    fn resonator_args(
        input: &std::path::Path,
        output: &std::path::Path,
        params: &[&str],
        automation: Option<&std::path::Path>,
    ) -> ProcessArgs {
        ProcessArgs {
            input_path: input.to_str().unwrap().to_string(),
            output_path: output.to_str().unwrap().to_string(),
            processor_name: "resonator".to_string(),
            params: params.iter().map(|p| p.to_string()).collect(),
            automation: automation.map(|p| p.to_str().unwrap().to_string()),
        }
    }

    #[test]
    fn resonator_renders_stereo_of_matching_length() {
        let out = run_on_ramp("resonator", &["root_hz=220", "damping=0.4"]);
        assert_eq!(out.len(), 2000 * 2);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn resonator_rejects_unknown_key_and_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let output = dir.path().join("out.wav");
        write_ramp(&input);
        let err = run_process(&resonator_args(&input, &output, &["bogus=1"], None)).unwrap_err();
        assert!(err.contains("bogus"), "{err}");
        let err =
            run_process(&resonator_args(&input, &output, &["root_hz=1000000"], None)).unwrap_err();
        assert!(err.contains("root_hz"), "{err}");
        let err = run_process(&resonator_args(&input, &output, &["damping=-5"], None)).unwrap_err();
        assert!(err.contains("damping"), "{err}");
    }

    #[test]
    fn resonator_accepts_boundary_values() {
        use asperitas_dsp::resonator_controls::RESONATOR_CONTROL_PARAMS;
        for spec in RESONATOR_CONTROL_PARAMS.iter() {
            for v in [spec.min, spec.max] {
                let out = run_on_ramp("resonator", &[&format!("{}={v}", spec.name)]);
                assert!(out.iter().all(|s| s.is_finite()), "{} = {v}", spec.name);
            }
        }
    }

    #[test]
    fn resonator_render_is_deterministic() {
        let a = run_on_ramp("resonator", &["root_hz=330", "structure=0.5", "spread=0.5"]);
        let b = run_on_ramp("resonator", &["root_hz=330", "structure=0.5", "spread=0.5"]);
        assert_eq!(a, b);
    }

    #[test]
    fn resonator_automation_latches_on_the_block_grid() {
        use asperitas_dsp::processor::Frame;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let auto = dir.path().join("a.txt");
        let output = dir.path().join("out.wav");
        write_ramp(&input);
        // 0.0105 s is frame 504: inside block 15 (frames 480..512), so it latches at 512.
        let script = "0 damping=0.2\n0.0105 step damping=0.9\n";
        std::fs::write(&auto, script).unwrap();
        run_process(&resonator_args(&input, &output, &[], Some(&auto))).unwrap();
        let rendered = read_f32(&output);

        let (_, input_frames) = wav_io::read_wav(input.to_str().unwrap()).unwrap();
        let automation = Automation::<ResonatorControls>::parse_for(script, 48_000).unwrap();
        let direct = |apply_at: &dyn Fn(usize) -> u64| -> Vec<f32> {
            let mut frames: Vec<Frame> = input_frames.clone();
            let mut bank = Resonator::new(48_000.0, &automation.controls_at(0).params());
            let mut start = 0;
            while start < frames.len() {
                let end = (start + 32).min(frames.len());
                bank.set_params(&automation.controls_at(apply_at(start)).params());
                bank.process_block(&mut frames[start..end]);
                start = end;
            }
            frames.iter().flat_map(|f| [f[0], f[1]]).collect()
        };
        assert_eq!(rendered, direct(&|block_start| block_start as u64));

        // Negative control: latching at the exact frame instead of the block start must differ.
        let exact = {
            let mut frames: Vec<Frame> = input_frames.clone();
            let mut bank = Resonator::new(48_000.0, &automation.controls_at(0).params());
            bank.set_params(&automation.controls_at(0).params());
            bank.process_block(&mut frames[..504]);
            bank.set_params(&automation.controls_at(504).params());
            bank.process_block(&mut frames[504..]);
            frames
                .iter()
                .flat_map(|f| [f[0], f[1]])
                .collect::<Vec<f32>>()
        };
        assert_ne!(rendered, exact);
    }

    #[test]
    fn resonator_automation_conflicts_and_refusals() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.wav");
        let auto = dir.path().join("a.txt");
        let output = dir.path().join("out.wav");
        write_ramp(&input);
        std::fs::write(&auto, "0 damping=0.2\n").unwrap();
        let err = run_process(&resonator_args(
            &input,
            &output,
            &["damping=0.3"],
            Some(&auto),
        ))
        .unwrap_err();
        assert!(err.contains("--params"), "{err}");
        let mut args = resonator_args(&input, &output, &[], Some(&auto));
        args.processor_name = "filter".to_string();
        assert!(run_process(&args).unwrap_err().contains("--automation"));
    }
}
