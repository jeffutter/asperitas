//! Golden-file regression tests for asperitas-cli DSP processors.
//!
//! Generates deterministic synthetic signals, processes them, and compares against
//! stored golden WAV files. Set `UPDATE_GOLDENS=1` to regenerate goldens explicitly.

use asperitas_cli::synth;
use asperitas_cli::wav_io;
use asperitas_dsp::filter::OnePoleLowPass;
use asperitas_dsp::gain::Gain;
use asperitas_dsp::processor::{Frame, Processor};

const GOLDEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../audio/goldens");
const SAMPLE_RATE: u32 = 48000;
const DURATION: f32 = 1.0;

/// Tolerance for per-sample comparison (absolute delta).
/// Accounts for double 16-bit quantization round-trip (write + read).
const TOLERANCE: f32 = 1e-4;

// -- Gain processor goldens --

fn gain_golden_path(signal: &str) -> String {
    format!("{GOLDEN_DIR}/gain_default_{signal}.wav")
}

fn run_gain_golden(signal: &str, golden_name: &str) {
    let mono = match signal {
        "impulse" => synth::generate_impulse(SAMPLE_RATE, DURATION),
        "sweep" => synth::generate_sweep(SAMPLE_RATE, DURATION),
        "pluck" => synth::generate_pluck(SAMPLE_RATE, DURATION, 440.0),
        _ => panic!("unknown signal: {signal}"),
    };
    let frames = synth::to_stereo(&mono);

    // Process with default gain params (0 dB = unity)
    let mut output = vec![Frame::default(); frames.len()];
    let params = asperitas_dsp::GainParams::default();
    let mut proc = Gain::default();
    proc.set_sample_rate(SAMPLE_RATE as f32);
    proc.set_params(&params);
    proc.process_block(&frames, &mut output);

    let path = gain_golden_path(signal);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    if std::env::var("UPDATE_GOLDENS").is_ok() {
        wav_io::write_wav(&path, spec, &output)
            .unwrap_or_else(|e| panic!("failed to write golden {golden_name}: {e}"));
        return;
    }

    compare_golden(&path, &output, golden_name);
}

#[test]
fn golden_gain_impulse() {
    run_gain_golden("impulse", "gain+impulse");
}

#[test]
fn golden_gain_sweep() {
    run_gain_golden("sweep", "gain+sweep");
}

#[test]
fn golden_gain_pluck() {
    run_gain_golden("pluck", "gain+pluck");
}

// -- Filter processor goldens --

fn filter_golden_path(signal: &str) -> String {
    format!("{GOLDEN_DIR}/filter_default_{signal}.wav")
}

fn run_filter_golden(signal: &str, golden_name: &str) {
    let mono = match signal {
        "impulse" => synth::generate_impulse(SAMPLE_RATE, DURATION),
        "sweep" => synth::generate_sweep(SAMPLE_RATE, DURATION),
        "pluck" => synth::generate_pluck(SAMPLE_RATE, DURATION, 440.0),
        _ => panic!("unknown signal: {signal}"),
    };
    let frames = synth::to_stereo(&mono);

    // Process with default filter params (20 kHz cutoff = pass-through)
    let mut output = vec![Frame::default(); frames.len()];
    let params = asperitas_dsp::FilterParams::default();
    let mut proc = OnePoleLowPass::default();
    proc.set_sample_rate(SAMPLE_RATE as f32);
    proc.set_params(&params);
    proc.process_block(&frames, &mut output);

    let path = filter_golden_path(signal);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    if std::env::var("UPDATE_GOLDENS").is_ok() {
        wav_io::write_wav(&path, spec, &output)
            .unwrap_or_else(|e| panic!("failed to write golden {golden_name}: {e}"));
        return;
    }

    compare_golden(&path, &output, golden_name);
}

#[test]
fn golden_filter_impulse() {
    run_filter_golden("impulse", "filter+impulse");
}

#[test]
fn golden_filter_sweep() {
    run_filter_golden("sweep", "filter+sweep");
}

#[test]
fn golden_filter_pluck() {
    run_filter_golden("pluck", "filter+pluck");
}

// -- Golden comparison helper --

fn compare_golden(path: &str, expected: &[Frame], label: &str) {
    if !std::path::Path::new(path).exists() {
        panic!(
            "golden file not found: {path}\n\
             Run `UPDATE_GOLDENS=1 cargo test -p asperitas-cli` to generate it."
        );
    }

    let (_, actual) = wav_io::read_wav(path)
        .unwrap_or_else(|e| panic!("cannot read golden {label} at {path}: {e}"));

    assert_eq!(
        actual.len(),
        expected.len(),
        "golden {label}: length mismatch (expected {} samples, got {})",
        expected.len(),
        actual.len()
    );

    let mut max_delta = 0.0f32;
    for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
        for ch in 0..2 {
            let delta = (exp[ch] - act[ch]).abs();
            if delta > max_delta {
                max_delta = delta;
            }
            let exp_val = exp[ch];
            let act_val = act[ch];
            assert!(
                delta < TOLERANCE,
                "golden {label}: sample [{i}] channel {ch} differs by {delta:.8} \
                 (expected {exp_val:.8}, got {act_val:.8}) (tolerance {TOLERANCE})"
            );
        }
    }

    if max_delta > 0.0 {
        eprintln!(
            "golden {label}: max sample delta = {max_delta:.2e} (within tolerance {TOLERANCE})"
        );
    }
}
