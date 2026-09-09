//! The device-side sweep generator must be the golden synthesiser, not a relative of it.
//!
//! `synth::generate_sweep` backs the golden files, so its chirp expression is frozen by
//! listening. `asperitas_dsp::ExponentialSweep` re-implements that expression for the firmware,
//! which shares no code with this crate. Two copies of one formula drift: the phase integral can
//! be grouped differently, the endpoints can be read as 20 Hz→20 kHz in one place and 20 Hz→19.9
//! kHz in another, and neither mistake shows up until a capture deconvolves into mush.
//!
//! So this compares them sample for sample over the region where both are defined identically —
//! everywhere except the 480-sample raised-cosine fades the device applies and the golden
//! synthesiser does not.
//!
//! ## Where the two are meant to differ
//!
//! 1. **Level.** `generate_sweep` is unit-amplitude; `ExponentialSweep` obeys `level_dbfs` and
//!    defaults to −20 dBFS. Every comparison here divides by each record's own measured peak, so
//!    level is checked separately rather than folded into shape.
//! 2. **Endpoints.** `generate_sweep` hardcodes 20 Hz → 20 kHz; the device source takes `f0_hz` and
//!    `f1_hz`. These tests pass the golden's endpoints in explicitly, which is what makes the
//!    comparison a comparison at all.
//! 3. **Length.** `generate_sweep` computes `(sample_rate_hz as f32 * duration_secs) as usize`, a
//!    truncating float multiply; the device source takes `total_samples` as an exact integer count.
//!    At 48000 × 1.0 the two agree, but a request for 0.1 s at 44.1 kHz can come out a sample short
//!    on that side. Left alone deliberately: `synth.rs` backs the goldens, and fixing its rounding
//!    would move them.
//! 4. **Fades.** The device record gets raised-cosine fades at both ends and the golden synthesiser
//!    gets none, which is why the shape check excludes the margins and a separate test asserts the
//!    margins are where the disagreement lives.
//! 5. **Impulse.** `synth::generate_impulse` is a one-sample delta and `PulseTrain` is a
//!    band-limited pulse. They are not compared, on purpose: the delta is flat to Nyquist by
//!    construction, which is the property AC #4 rejects for a device stimulus because it puts energy
//!    exactly where the analog loop is least trustworthy. The delta keeps its place as a golden-path
//!    impulse probe; nothing here should replace it.

use asperitas_cli::synth;
use asperitas_dsp::processor::Processor;
use asperitas_dsp::{ExponentialSweep, ExponentialSweepParams};

const SAMPLE_RATE: u32 = 48_000;
const DURATION_SECS: f32 = 1.0;
/// Raised-cosine fade at each end of the device record, mirrored from `asperitas-dsp`.
///
/// A copy rather than an import because it is private there on purpose: it is part of what the
/// `describe()` wire format promises, not a knob. If it changes, this test's window moves and the
/// pinned `describe()` strings have to move too, which is the loud failure this arrangement wants.
const FADE_SAMPLES: usize = 480;

/// Largest difference seen between two unit-amplitude chirps over the untapered interior.
///
/// Set from measurement: the two implementations group the phase integral differently
/// (`ω0·T·(r^u − 1)/ln r` here against `ω0·T/ln r · (r^u − 1)` there), which costs about 6e-8 once
/// the result lands in `f32`. Anything approaching this budget would mean the expressions
/// themselves had diverged, not merely their rounding.
const TOLERANCE: f64 = 1e-6;

fn golden() -> Vec<f32> {
    synth::generate_sweep(SAMPLE_RATE, DURATION_SECS)
}

/// Play the device generator with the golden synthesiser's parameters.
fn device(level_dbfs: f32) -> Vec<f32> {
    let mut source = ExponentialSweep::default();
    source.set_sample_rate(SAMPLE_RATE as f32);
    source.set_params(&ExponentialSweepParams {
        level_dbfs,
        f0_hz: 20.0,
        f1_hz: 20_000.0,
        total_samples: SAMPLE_RATE,
    });
    (0..SAMPLE_RATE as usize)
        .map(|_| source.tick([0.0, 0.0])[0])
        .collect()
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |a, &v| a.max(v.abs()))
}

/// Worst absolute difference after putting both records on the same unit-amplitude footing.
///
/// Dividing by each record's own peak rather than assuming one removes the question of who
/// normalises to what, and leaves only shape.
fn worst_interior_difference(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len());
    let pa = f64::from(peak(a));
    let pb = f64::from(peak(b));
    (FADE_SAMPLES..a.len() - FADE_SAMPLES)
        .map(|i| (f64::from(a[i]) / pa - f64::from(b[i]) / pb).abs())
        .fold(0.0f64, f64::max)
}

#[test]
fn device_sweep_has_the_golden_sweeps_shape() {
    let reference = golden();
    let played = device(0.0);
    assert_eq!(
        played.len(),
        reference.len(),
        "one second of sweep must be the same length on both sides"
    );

    let worst = worst_interior_difference(&played, &reference);
    assert!(
        worst < TOLERANCE,
        "interior samples disagree by {worst:.3e}, budget is {TOLERANCE:.1e}"
    );

    // Peak level is the scaling contract: at 0 dBFS the device record tops out at exactly full
    // scale, which is why the comparison above could divide by the measured peak at all.
    assert!(
        (f64::from(peak(&played)) - 1.0).abs() < 1e-9,
        "0 dBFS produced a peak of {}",
        f64::from(peak(&played))
    );
}

#[test]
fn device_sweep_level_is_the_golden_sweep_scaled_by_dbfs() {
    // Shape agreement alone would pass if the device scaled by, say, amplitude squared. This pins
    // the calibration: a −20 dBFS record is the golden chirp at 0.1, not merely proportional to it.
    let reference = golden();
    let played = device(-20.0);
    let wanted = f64::from(10f32.powf(-20.0 / 20.0));
    let reference_peak = f64::from(peak(&reference));

    let worst = (FADE_SAMPLES..reference.len() - FADE_SAMPLES)
        .map(|i| (f64::from(played[i]) - f64::from(reference[i]) * wanted / reference_peak).abs())
        .fold(0.0f64, f64::max);
    assert!(
        worst < TOLERANCE,
        "−20 dBFS record differs from the golden chirp at 0.1 by {worst:.3e}"
    );
}

#[test]
fn the_fades_are_the_only_disagreement() {
    // The interior comparison excludes the fades, so say what lives inside them. Measured at these
    // parameters: the untapered golden differs from the faded device record by 0.32 across the first
    // 480 samples and 1.00 across the last, against 6e-8 everywhere between. That ratio is the
    // point — if the two ever agree in the margins, or disagree throughout them, the exclusion in
    // the tests above has stopped describing two windows and started hiding something.
    let reference = golden();
    let played = device(0.0);
    let n = reference.len();

    let head_difference = (0..FADE_SAMPLES)
        .map(|i| (f64::from(played[i]) - f64::from(reference[i])).abs())
        .fold(0.0f64, f64::max);
    let tail_difference = (n - FADE_SAMPLES..n)
        .map(|i| (f64::from(played[i]) - f64::from(reference[i])).abs())
        .fold(0.0f64, f64::max);
    let interior_difference = worst_interior_difference(&played, &reference);

    assert!(
        head_difference > 0.1 && tail_difference > 0.5,
        "fades no longer move the waveform: head {head_difference:.3}, tail {tail_difference:.3}"
    );
    assert!(
        head_difference > 1e3 * interior_difference && tail_difference > 1e3 * interior_difference,
        "margin disagreement ({head_difference:.3}, {tail_difference:.3}) is no longer confined \
         against an interior of {interior_difference:.3e}"
    );

    // And the record closes on silence rather than mid-swing, which is what Farina's AES 122 note
    // asks for: a step at the end smears energy across the whole spectrum.
    assert_eq!(played[0], 0.0, "device record no longer opens on silence");
    assert_eq!(
        played[n - 1],
        0.0,
        "device record no longer closes on silence; the fade-out is gone"
    );
}
