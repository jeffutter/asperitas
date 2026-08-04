//! Process a WAV file through a named DSP processor.

use asperitas_dsp::filter::OnePoleLowPass;
use asperitas_dsp::gain::Gain;
use asperitas_dsp::processor::{Frame, Processor};

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

    let mut output = vec![Frame::default(); frames.len()];

    match args.processor_name.as_str() {
        "gain" => {
            let params = Gain::parse_params_from_cli(&parse_param_pairs(&args.params)?)?;
            let sample_rate = spec.sample_rate as f32;
            let mut proc = Gain::default();
            proc.set_sample_rate(sample_rate);
            proc.set_params(&params);
            proc.process_block(&frames, &mut output);
        }
        "filter" => {
            let params = OnePoleLowPass::parse_params_from_cli(&parse_param_pairs(&args.params)?)?;
            let sample_rate = spec.sample_rate as f32;
            let mut proc = OnePoleLowPass::default();
            proc.set_sample_rate(sample_rate);
            proc.set_params(&params);
            proc.process_block(&frames, &mut output);
        }
        other => {
            return Err(format!(
                "unknown processor '{}'; supported: gain, filter",
                other
            ))
        }
    }

    // Output is always stereo regardless of the input's channel count; only the rate
    // carries over from the input file.
    wav_io::write_wav(&args.output_path, spec.sample_rate, &output)?;
    Ok(())
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
}
