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

    wav_io::write_wav(&args.output_path, spec, &output)?;
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
}
