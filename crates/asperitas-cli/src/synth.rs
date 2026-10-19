//! Synthetic signal generators for testing and golden-file regression.
//!
//! All generators are deterministic and produce mono f32 samples at a given sample rate.

use asperitas_dsp::processor::Frame;
use asperitas_stimulus::Chirp;

/// Samples in `duration_secs` at `sample_rate_hz`, rounded to the nearest whole sample.
///
/// Rounded in `f64` rather than truncated in `f32`, which comes out a sample short whenever the
/// product is not exactly representable.
fn sample_count(sample_rate_hz: u32, duration_secs: f32) -> u32 {
    (f64::from(sample_rate_hz) * f64::from(duration_secs)).round() as u32
}

/// Generate a unit impulse (single sample = 1.0, rest zeros).
pub fn generate_impulse(sample_rate_hz: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = sample_count(sample_rate_hz, duration_secs) as usize;
    let mut buf = vec![0.0f32; total_samples];
    if !buf.is_empty() {
        buf[0] = 1.0;
    }
    buf
}

/// Generate a logarithmic sine sweep from 20 Hz to 20 kHz at unit amplitude.
///
/// The chirp is `asperitas_stimulus::Chirp`, the same expression the device sweep plays, so the
/// golden files and the rig cannot disagree about the waveform. It evaluates the phase in `f64`
/// through `libm` rather than in `f32` through `std`: the phase integral reaches ~18,000 radians
/// by the end of a one-second sweep, where an `f32` ULP (~1.1e-3) is over ten times the goldens'
/// comparison tolerance, and the platform libm differs between macOS and glibc. The golden files
/// would otherwise record one machine's rounding.
pub fn generate_sweep(sample_rate_hz: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = sample_count(sample_rate_hz, duration_secs);
    if total_samples == 0 {
        return Vec::new();
    }
    let chirp = Chirp::new(20.0, 20_000.0, total_samples, sample_rate_hz);
    (0..total_samples as usize)
        .map(|n| chirp.sample(n) as f32)
        .collect()
}

/// Generate a Karplus-Strong plucked-string sound at the given frequency.
///
/// Uses a deterministic LCG-seeded noise buffer for reproducibility.
pub fn generate_pluck(sample_rate_hz: u32, duration_secs: f32, frequency_hz: f32) -> Vec<f32> {
    let total_samples = sample_count(sample_rate_hz, duration_secs) as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_count_rounds_instead_of_truncating() {
        // 44_100 * 0.1f32 is 4409.9999..., which truncation reads as 4409.
        assert_eq!(sample_count(44_100, 0.1), 4410);
        assert_eq!(generate_sweep(44_100, 0.1).len(), 4410);
        assert_eq!(sample_count(48_000, 1.0), 48_000);
    }
}
