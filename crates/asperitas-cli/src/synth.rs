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
///
/// The phase is accumulated in `f64` via `libm`, not `f32` via `std`, because this
/// signal backs golden-file regression tests and so must be bit-reproducible.
///
/// Two separate things would otherwise prevent that:
///
/// 1. **Magnitude.** The chirp's phase integral reaches ~18,000 radians by the end of a
///    one-second sweep. An `f32` ULP at that magnitude is ~1.1e-3 — over ten times the
///    goldens' 1e-4 comparison tolerance — so the sample values there are decided
///    entirely by rounding. In `f64` the ULP is ~3.6e-12 and the phase is effectively
///    exact.
/// 2. **Implementation.** `f32::powf`, `f32::ln`, and `f32::sin` dispatch to the
///    platform's libm, which differs between macOS and glibc and can shift across
///    toolchain versions. `libm` is a pure-Rust port of MUSL's implementations and is
///    identical everywhere. `asperitas-dsp` already uses it for this reason.
///
/// Together those made the goldens a record of one machine's rounding behaviour. The
/// fenix -> rust-overlay toolchain switch alone moved samples by 4 LSB at 16 bits and
/// broke the tests without any DSP change.
pub fn generate_sweep(sample_rate_hz: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = (sample_rate_hz as f32 * duration_secs) as usize;
    let mut buf = vec![0.0f32; total_samples];

    let f_start = 20.0f64;
    let f_end = 20_000.0f64;
    let t = duration_secs as f64;
    let omega_start = 2.0 * core::f64::consts::PI * f_start;
    let omega_end = 2.0 * core::f64::consts::PI * f_end;
    let ratio = omega_end / omega_start;
    let log_ratio = libm::log(ratio);

    for (i, sample) in buf.iter_mut().enumerate() {
        let ti = i as f64 / sample_rate_hz as f64;
        // Phase integral of log chirp: phi(t) = omega_start * T * ((r^(t/T) - 1) / ln(r))
        let phase = omega_start * t * ((libm::pow(ratio, ti / t) - 1.0) / log_ratio);
        *sample = libm::sin(phase) as f32;
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
