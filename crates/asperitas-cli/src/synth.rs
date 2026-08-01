//! Synthetic signal generators for testing and golden-file regression.
//!
//! All generators are deterministic and produce mono f32 samples at a given sample rate.

use asperitas_dsp::processor::Frame;

/// Generate a unit impulse (single sample = 1.0, rest zeros).
pub fn generate_impulse(sample_rate_hz: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = (sample_rate_hz as f32 * duration_secs) as usize;
    let mut buf = vec![0.0f32; total_samples];
    if !buf.is_empty() {
        buf[0] = 1.0;
    }
    buf
}

/// Generate a logarithmic sine sweep from 20 Hz to 20 kHz.
pub fn generate_sweep(sample_rate_hz: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = (sample_rate_hz as f32 * duration_secs) as usize;
    let mut buf = vec![0.0f32; total_samples];

    let f_start = 20.0f32;
    let f_end = 20_000.0f32;
    let t = duration_secs;
    let omega_start = 2.0 * core::f32::consts::PI * f_start;
    let omega_end = 2.0 * core::f32::consts::PI * f_end;
    let ratio = omega_end / omega_start;
    let log_ratio = ratio.ln();

    for (i, sample) in buf.iter_mut().enumerate() {
        let ti = i as f32 / sample_rate_hz as f32;
        // Phase integral of log chirp: phi(t) = omega_start * T * ((exp(r*T/t) - 1) / ln(r))
        let phase = omega_start * t * ((ratio.powf(ti / t) - 1.0) / log_ratio);
        *sample = phase.sin();
    }

    buf
}

/// Generate a Karplus-Strong plucked-string sound at the given frequency.
///
/// Uses a deterministic LCG-seeded noise buffer for reproducibility.
pub fn generate_pluck(sample_rate_hz: u32, duration_secs: f32, frequency_hz: f32) -> Vec<f32> {
    let total_samples = (sample_rate_hz as f32 * duration_secs) as usize;
    let delay_samples = (sample_rate_hz as f32 / frequency_hz.max(20.0)).round() as usize;
    let delay_samples = delay_samples.max(1).min(total_samples);

    // Initialize buffer with deterministic LCG noise
    let mut buf = vec![0.0f32; delay_samples];
    let mut seed: u64 = 12345;
    for val in &mut buf {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *val = (seed >> 33) as f32 / u32::MAX as f32 * 2.0 - 1.0;
    }

    let damping = 0.996f32;

    let mut output = vec![0.0f32; total_samples];
    let mut write_idx = 0usize;

    for (i, out_sample) in output.iter_mut().enumerate() {
        let read_idx = (i + 1) % delay_samples;
        let next_read = (read_idx + 1) % delay_samples;
        let new_val = damping * 0.5 * (buf[read_idx] + buf[next_read]);
        *out_sample = buf[write_idx];
        buf[write_idx] = new_val;
        write_idx = (write_idx + 1) % delay_samples;
    }

    output
}

/// Convert a mono sample buffer to stereo frames by duplicating the channel.
pub fn to_stereo(mono: &[f32]) -> Vec<Frame> {
    mono.iter().map(|&s| [s, s]).collect()
}
