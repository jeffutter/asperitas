//! Process a WAV file through a named DSP processor.

use asperitas_dsp::asperitas::{Asperitas, Controls, ASPERITAS_PARAMS};
use asperitas_dsp::filter::{FilterParams, OnePoleLowPass, FILTER_PARAMS};
use asperitas_dsp::gain::{Gain, GainParams, GAIN_PARAMS};
use asperitas_dsp::param::ParamSpec;
use asperitas_dsp::processor::Processor;

use crate::wav_io;

/// Arguments for the process subcommand.
pub struct ProcessArgs {
    pub input_path: String,
    pub output_path: String,
    pub processor_name: String,
    pub params: Vec<String>,
}

/// Run the process command: read input WAV, process it, write output WAV.
pub fn run_process(args: &ProcessArgs) -> Result<(), String> {
    let (spec, frames) = wav_io::read_wav(&args.input_path)?;

    let mut frames = frames;

    let sample_rate = spec.sample_rate as f32;
    let pairs = parse_param_pairs(&args.params)?;
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
            // Unlike gain and filter, the effect takes knob positions, which clamp; so range
            // check the real-unit values here instead of silently moving them.
            let values = parse_with_table("asperitas", &ASPERITAS_PARAMS, &pairs)?;
            for (spec, value) in ASPERITAS_PARAMS.iter().zip(&values) {
                if !value.is_finite() || *value < spec.min || *value > spec.max {
                    return Err(format!(
                        "{} out of range for asperitas processor: {value} (allowed {}..={} {})",
                        spec.name, spec.min, spec.max, spec.unit
                    ));
                }
            }
            let mut controls = Controls::default();
            for ((knob, spec), value) in
                controls.knobs.iter_mut().zip(&ASPERITAS_PARAMS).zip(values)
            {
                *knob = spec.to_normalised(value);
            }
            Asperitas::new(sample_rate, &controls).process_block(&mut frames);
        }
        other => {
            return Err(format!(
                "unknown processor '{other}'; supported: gain, filter, asperitas"
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
        let err = parse_with_table(
            "asperitas",
            &ASPERITAS_PARAMS,
            &[("bogus".into(), "1".into())],
        )
        .unwrap_err();
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
}
