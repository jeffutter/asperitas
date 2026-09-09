//! Property-based tests for any `Processor` implementation.
//!
//! Run with: `cargo test -p asperitas-dsp`

use asperitas_dsp::{
    ExponentialSweep, ExponentialSweepParams, FilterParams, Frame, Gain, GainParams,
    OnePoleLowPass, Processor, PulseTrain, PulseTrainParams, Sine, SineParams,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a block of frames (power-of-2 lengths matching typical DMA sizes).
fn arb_block() -> impl Strategy<Value = Vec<Frame>> {
    let frame = (-1.0f32..=1.0).prop_flat_map(|l| (-1.0f32..=1.0).prop_map(move |r| [l, r]));
    prop_oneof![
        prop::collection::vec(frame.clone(), 4..64),
        prop::collection::vec(frame.clone(), 64..256),
        prop::collection::vec(frame.clone(), 256..512),
    ]
}

/// A small positive epsilon for float comparisons.
const EPSILON: f32 = 1e-6;

// ---------------------------------------------------------------------------
// Gain property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn gain_output_always_finite(block in arb_block()) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams::default());
        let mut out = vec![Frame::default(); block.len()];
        g.process_block(&block, &mut out);
        for f in &out {
            prop_assert!(f[0].is_finite(), "left not finite: {}", f[0]);
            prop_assert!(f[1].is_finite(), "right not finite: {}", f[1]);
        }
    }

    #[test]
    fn gain_output_bounded(block in arb_block()) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams::default());
        let mut out = vec![Frame::default(); block.len()];
        g.process_block(&block, &mut out);
        for f in &out {
            prop_assert!(f[0].abs() <= 1.0 + EPSILON, "left unbounded: {}", f[0]);
            prop_assert!(f[1].abs() <= 1.0 + EPSILON, "right unbounded: {}", f[1]);
        }
    }

    #[test]
    fn gain_silence_in_silence_out(block_size in 4usize..256) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams::default());

        // Warm up with arbitrary signal
        let warmup: Vec<Frame> = (0..block_size * 2)
            .map(|i| [(i as f32 % 100.0 - 50.0) / 50.0, -(i as f32 % 100.0 - 50.0) / 50.0])
            .collect();
        let mut wo = vec![Frame::default(); warmup.len()];
        g.process_block(&warmup, &mut wo);

        // Feed silence
        let silence = vec![Frame::default(); block_size.max(512)];
        let mut out = vec![Frame::default(); silence.len()];
        g.process_block(&silence, &mut out);

        for f in &out {
            prop_assert!(f[0].abs() < EPSILON, "left not silent: {}", f[0]);
            prop_assert!(f[1].abs() < EPSILON, "right not silent: {}", f[1]);
        }
    }

    #[test]
    fn gain_reset_idempotent(block in arb_block()) {
        let mut ga = Gain::default();
        let mut gb = Gain::default();
        ga.set_sample_rate(48_000.0);
        gb.set_sample_rate(48_000.0);

        // Dirty state
        let junk: Vec<Frame> = (0..64).map(|i| [(i as f32 / 32.0 - 1.0), -(i as f32 / 32.0 - 1.0)]).collect();
        let mut jo = vec![Frame::default(); junk.len()];
        ga.process_block(&junk, &mut jo);
        gb.process_block(&junk, &mut jo);

        ga.reset();
        gb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        ga.process_block(&block, &mut oa);
        gb.process_block(&block, &mut ob);

        for (i, (fa, fb)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert!((fa[0] - fb[0]).abs() < EPSILON, "left mismatch at sample {}: {} != {}", i, fa[0], fb[0]);
            prop_assert!((fa[1] - fb[1]).abs() < EPSILON, "right mismatch at sample {}: {} != {}", i, fa[1], fb[1]);
        }
    }

    #[test]
    fn gain_block_equals_tick(block in arb_block()) {
        let n = block.len();
        if n == 0 { return Ok(()); }

        let mut gb = Gain::default();
        gb.set_sample_rate(48_000.0);
        gb.set_params(&GainParams::default());
        let mut ob = vec![Frame::default(); n];
        gb.process_block(&block, &mut ob);

        let mut gt = Gain::default();
        gt.set_sample_rate(48_000.0);
        gt.set_params(&GainParams::default());
        let ot: Vec<Frame> = block.iter().map(|f| gt.tick(*f)).collect();

        for (i, (a, b)) in ob.iter().zip(ot.iter()).enumerate() {
            prop_assert!((a[0] - b[0]).abs() < EPSILON, "left mismatch at sample {}: {} != {}", i, a[0], b[0]);
            prop_assert!((a[1] - b[1]).abs() < EPSILON, "right mismatch at sample {}: {} != {}", i, a[1], b[1]);
        }
    }

    #[test]
    fn gain_param_change_smooth(gain_db_a in -30.0f32..=30.0, gain_db_b in -30.0f32..=30.0) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams { gain_db: gain_db_a });

        // Warm up so smoothing converges
        let warmup = vec![Frame::from([0.5, 0.5]); 256];
        let mut wo = vec![Frame::default(); warmup.len()];
        g.process_block(&warmup, &mut wo);

        // Change params mid-stream with constant input
        g.set_params(&GainParams { gain_db: gain_db_b });
        let out = g.tick(Frame::from([0.5, 0.5]));

        // The jump from last warmup sample to post-change should be modest
        // (smoothing prevents clicks). Allow up to 5% of full-scale transition.
        let delta_l = (out[0] - wo.last().unwrap()[0]).abs();
        let delta_r = (out[1] - wo.last().unwrap()[1]).abs();
        prop_assert!(delta_l < 0.05, "left discontinuity too large: {}", delta_l);
        prop_assert!(delta_r < 0.05, "right discontinuity too large: {}", delta_r);
    }
}

// ---------------------------------------------------------------------------
// Filter property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn filter_output_always_finite(block in arb_block()) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams::default());
        let mut out = vec![Frame::default(); block.len()];
        f.process_block(&block, &mut out);
        for frame in &out {
            prop_assert!(frame[0].is_finite(), "left not finite: {}", frame[0]);
            prop_assert!(frame[1].is_finite(), "right not finite: {}", frame[1]);
        }
    }

    #[test]
    fn filter_output_bounded(block in arb_block()) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams::default());
        let mut out = vec![Frame::default(); block.len()];
        f.process_block(&block, &mut out);
        for frame in &out {
            prop_assert!(frame[0].abs() <= 1.0 + EPSILON, "left unbounded: {}", frame[0]);
            prop_assert!(frame[1].abs() <= 1.0 + EPSILON, "right unbounded: {}", frame[1]);
        }
    }

    #[test]
    fn filter_silence_in_silence_out(block_size in 4usize..256) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams::default());

        // Warm up
        let warmup: Vec<Frame> = (0..block_size * 2)
            .map(|i| [(i as f32 % 100.0 - 50.0) / 50.0, -(i as f32 % 100.0 - 50.0) / 50.0])
            .collect();
        let mut wo = vec![Frame::default(); warmup.len()];
        f.process_block(&warmup, &mut wo);

        // Feed extended silence.
        // At default 20 kHz cutoff with 48 kHz sample rate, alpha ≈ 0.999,
        // so the time constant is ~7500 samples. We need ~5× that to decay.
        let silence_len = 50_000;
        let silence = vec![Frame::default(); silence_len];
        let mut out = vec![Frame::default(); silence_len];
        f.process_block(&silence, &mut out);

        // Check the tail end (last quarter) has decayed to near-zero
        let tail_start = silence_len - silence_len / 4;
        for (i, frame) in out[tail_start..].iter().enumerate() {
            prop_assert!(
                frame[0].abs() < 1e-4 && frame[1].abs() < 1e-4,
                "silence not achieved at tail sample {}: [{}, {}]",
                tail_start + i, frame[0], frame[1]
            );
        }
    }

    #[test]
    fn filter_reset_idempotent(block in arb_block()) {
        let mut fa = OnePoleLowPass::default();
        let mut fb = OnePoleLowPass::default();
        fa.set_sample_rate(48_000.0);
        fb.set_sample_rate(48_000.0);

        let junk: Vec<Frame> = (0..64).map(|i| [(i as f32 / 32.0 - 1.0), -(i as f32 / 32.0 - 1.0)]).collect();
        let mut jo = vec![Frame::default(); junk.len()];
        fa.process_block(&junk, &mut jo);
        fb.process_block(&junk, &mut jo);

        fa.reset();
        fb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        fa.process_block(&block, &mut oa);
        fb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert!((a[0] - b[0]).abs() < EPSILON, "left mismatch at sample {}: {} != {}", i, a[0], b[0]);
            prop_assert!((a[1] - b[1]).abs() < EPSILON, "right mismatch at sample {}: {} != {}", i, a[1], b[1]);
        }
    }

    #[test]
    fn filter_block_equals_tick(block in arb_block()) {
        let n = block.len();
        if n == 0 { return Ok(()); }

        let mut fb = OnePoleLowPass::default();
        fb.set_sample_rate(48_000.0);
        fb.set_params(&FilterParams::default());
        let mut ob = vec![Frame::default(); n];
        fb.process_block(&block, &mut ob);

        let mut ft = OnePoleLowPass::default();
        ft.set_sample_rate(48_000.0);
        ft.set_params(&FilterParams::default());
        let ot: Vec<Frame> = block.iter().map(|f| ft.tick(*f)).collect();

        for (i, (a, b)) in ob.iter().zip(ot.iter()).enumerate() {
            prop_assert!((a[0] - b[0]).abs() < EPSILON, "left mismatch at sample {}: {} != {}", i, a[0], b[0]);
            prop_assert!((a[1] - b[1]).abs() < EPSILON, "right mismatch at sample {}: {} != {}", i, a[1], b[1]);
        }
    }

    #[test]
    fn filter_param_change_smooth(cutoff_a in 100.0f32..=20000.0, cutoff_b in 100.0f32..=20000.0) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams { cutoff_hz: cutoff_a });

        // Warm up so smoothing converges
        let warmup = vec![Frame::from([0.5, 0.5]); 256];
        let mut wo = vec![Frame::default(); warmup.len()];
        f.process_block(&warmup, &mut wo);

        // Change params mid-stream
        f.set_params(&FilterParams { cutoff_hz: cutoff_b });
        let out = f.tick(Frame::from([0.5, 0.5]));

        let delta_l = (out[0] - wo.last().unwrap()[0]).abs();
        let delta_r = (out[1] - wo.last().unwrap()[1]).abs();
        prop_assert!(delta_l < 0.05, "left discontinuity too large: {}", delta_l);
        prop_assert!(delta_r < 0.05, "right discontinuity too large: {}", delta_r);
    }
}

// ---------------------------------------------------------------------------
// Stimulus property tests
// ---------------------------------------------------------------------------

// Two of the six house properties are deliberately not offered for these three sources, and the
// omissions are decisions rather than gaps:
//
// - `*_param_change_smooth` — stimulus parameters are frozen for the duration of a capture, and a
//   smoothed level ramp would corrupt the calibration the capture exists to measure. The generators
//   apply parameters immediately, so there is nothing to smooth.
// - `*_silence_in_silence_out` — these sources ignore their input frames, so the property holds for
//   any generator that ignores its input, including one that does so by mistake.
//
// Where gain and filter compare outputs to `EPSILON`, these compare `.to_bits()`. Bit identity is
// the actual claim: firmware and host link this source and the analysis subtracts a reproduction of
// what was played, so a one-LSB disagreement between two runs is a defect, not a tolerance.
//
// Parameter ranges stay small on purpose. `ExponentialSweep` scans the whole record to find its
// true peak — once when constructed, because `default()` applies the default eight-second
// parameters, and again on every `set_params` — and proptest runs each property 256 times. Small
// parameter ranges remove the second cost but not the first, which is why the five `ess_` properties
// still take about a minute in a debug build.

/// Sweep parameters that keep the peak scan cheap without shrinking what it varies.
fn arb_ess_params() -> impl Strategy<Value = ExponentialSweepParams> {
    (
        -144.0f32..0.0,
        20.0f32..20_000.0,
        20.0f32..24_000.0,
        1u32..4_000,
    )
        .prop_map(
            |(level_dbfs, f0_hz, f1_hz, total_samples)| ExponentialSweepParams {
                level_dbfs,
                f0_hz,
                f1_hz,
                total_samples,
            },
        )
}

/// Pulse parameters, likewise bounded so a property run stays a property run.
fn arb_pulse_params() -> impl Strategy<Value = PulseTrainParams> {
    (-144.0f32..0.0, 2u32..4_000, 1.0f32..24_000.0).prop_map(
        |(level_dbfs, period_samples, max_frequency_hz)| PulseTrainParams {
            level_dbfs,
            period_samples,
            max_frequency_hz,
        },
    )
}

proptest! {
    #[test]
    fn sine_output_always_finite(block in arb_block(), freq in 1u32..24_000, level in -144.0f32..0.0) {
        let mut s = Sine::default();
        s.set_sample_rate(48_000.0);
        s.set_params(&SineParams { level_dbfs: level, frequency_hz: freq });
        let mut out = vec![Frame::default(); block.len()];
        s.process_block(&block, &mut out);
        for f in &out {
            prop_assert!(f[0].is_finite(), "left not finite: {}", f[0]);
            prop_assert!(f[1].is_finite(), "right not finite: {}", f[1]);
        }
    }

    #[test]
    fn sine_output_bounded(block in arb_block(), freq in 1u32..24_000, level in -144.0f32..0.0) {
        let mut s = Sine::default();
        s.set_sample_rate(48_000.0);
        s.set_params(&SineParams { level_dbfs: level, frequency_hz: freq });
        let mut out = vec![Frame::default(); block.len()];
        s.process_block(&block, &mut out);
        // Bounded twice over: never past full scale, and never past the requested peak. The second
        // bound is the one that catches an amplitude that wanders while staying legal.
        let ceiling = libm::pow(10.0, f64::from(level) / 20.0) as f32 * (1.0 + EPSILON);
        for f in &out {
            prop_assert!(f[0].abs() <= 1.0 + EPSILON, "left unbounded: {}", f[0]);
            prop_assert!(f[1].abs() <= 1.0 + EPSILON, "right unbounded: {}", f[1]);
            prop_assert!(f[0].abs() <= ceiling, "left exceeds {:?}: {}", ceiling, f[0]);
            prop_assert!(f[1].abs() <= ceiling, "right exceeds {:?}: {}", ceiling, f[1]);
        }
    }

    #[test]
    fn sine_reset_idempotent(block in arb_block(), freq in 1u32..24_000) {
        let mut sa = Sine::default();
        let mut sb = Sine::default();
        sa.set_sample_rate(48_000.0);
        sb.set_sample_rate(48_000.0);
        sa.set_params(&SineParams { level_dbfs: -20.0, frequency_hz: freq });
        sb.set_params(&SineParams { level_dbfs: -20.0, frequency_hz: freq });

        let junk: Vec<Frame> = (0..64).map(|i| [(i as f32 / 32.0 - 1.0), -(i as f32 / 32.0 - 1.0)]).collect();
        let mut jo = vec![Frame::default(); junk.len()];
        sa.process_block(&junk, &mut jo);
        sb.process_block(&junk, &mut jo);

        sa.reset();
        sb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        sa.process_block(&block, &mut oa);
        sb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn sine_block_equals_tick(block in arb_block(), freq in 1u32..24_000) {
        let n = block.len();
        if n == 0 { return Ok(()); }

        let mut fb = Sine::default();
        fb.set_sample_rate(48_000.0);
        fb.set_params(&SineParams { level_dbfs: -20.0, frequency_hz: freq });
        let mut ob = vec![Frame::default(); n];
        fb.process_block(&block, &mut ob);

        let mut ft = Sine::default();
        ft.set_sample_rate(48_000.0);
        ft.set_params(&SineParams { level_dbfs: -20.0, frequency_hz: freq });
        let ot: Vec<Frame> = block.iter().map(|f| ft.tick(*f)).collect();

        for (i, (a, b)) in ob.iter().zip(ot.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn sine_bit_identical_determinism(block in arb_block(), freq in 1u32..24_000, level in -144.0f32..0.0) {
        let params = SineParams { level_dbfs: level, frequency_hz: freq };

        let mut sa = Sine::default();
        sa.set_sample_rate(48_000.0);
        sa.set_params(&params);
        let mut oa = vec![Frame::default(); block.len()];
        sa.process_block(&block, &mut oa);

        // Built the same way but later, with no shared state beyond the parameter struct.
        let mut sb = Sine::default();
        sb.set_sample_rate(48_000.0);
        sb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        sb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }
}

proptest! {
    #[test]
    fn ess_output_always_finite(block in arb_block(), params in arb_ess_params()) {
        let mut e = ExponentialSweep::default();
        e.set_sample_rate(48_000.0);
        e.set_params(&params);
        let mut out = vec![Frame::default(); block.len()];
        e.process_block(&block, &mut out);
        for f in &out {
            prop_assert!(f[0].is_finite(), "left not finite: {}", f[0]);
            prop_assert!(f[1].is_finite(), "right not finite: {}", f[1]);
        }
    }

    #[test]
    fn ess_output_bounded(block in arb_block(), params in arb_ess_params()) {
        let mut e = ExponentialSweep::default();
        e.set_sample_rate(48_000.0);
        e.set_params(&params);
        let mut out = vec![Frame::default(); block.len()];
        e.process_block(&block, &mut out);
        let ceiling = libm::pow(10.0, f64::from(params.level_dbfs) / 20.0) as f32 * (1.0 + EPSILON);
        for f in &out {
            prop_assert!(f[0].abs() <= 1.0 + EPSILON, "left unbounded: {}", f[0]);
            prop_assert!(f[1].abs() <= 1.0 + EPSILON, "right unbounded: {}", f[1]);
            prop_assert!(f[0].abs() <= ceiling, "left exceeds {:?}: {}", ceiling, f[0]);
            prop_assert!(f[1].abs() <= ceiling, "right exceeds {:?}: {}", ceiling, f[1]);
        }
    }

    #[test]
    fn ess_reset_idempotent(block in arb_block(), params in arb_ess_params()) {
        let mut ea = ExponentialSweep::default();
        let mut eb = ExponentialSweep::default();
        ea.set_sample_rate(48_000.0);
        eb.set_sample_rate(48_000.0);
        ea.set_params(&params);
        eb.set_params(&params);

        let junk: Vec<Frame> = (0..64).map(|i| [(i as f32 / 32.0 - 1.0), -(i as f32 / 32.0 - 1.0)]).collect();
        let mut jo = vec![Frame::default(); junk.len()];
        ea.process_block(&junk, &mut jo);
        eb.process_block(&junk, &mut jo);

        ea.reset();
        eb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        ea.process_block(&block, &mut oa);
        eb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn ess_block_equals_tick(block in arb_block(), params in arb_ess_params()) {
        let n = block.len();
        if n == 0 { return Ok(()); }

        let mut fb = ExponentialSweep::default();
        fb.set_sample_rate(48_000.0);
        fb.set_params(&params);
        let mut ob = vec![Frame::default(); n];
        fb.process_block(&block, &mut ob);

        let mut ft = ExponentialSweep::default();
        ft.set_sample_rate(48_000.0);
        ft.set_params(&params);
        let ot: Vec<Frame> = block.iter().map(|f| ft.tick(*f)).collect();

        for (i, (a, b)) in ob.iter().zip(ot.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn ess_bit_identical_determinism(block in arb_block(), params in arb_ess_params()) {
        let mut ea = ExponentialSweep::default();
        ea.set_sample_rate(48_000.0);
        ea.set_params(&params);
        let mut oa = vec![Frame::default(); block.len()];
        ea.process_block(&block, &mut oa);

        let mut eb = ExponentialSweep::default();
        eb.set_sample_rate(48_000.0);
        eb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        eb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }
}

proptest! {
    #[test]
    fn pulse_output_always_finite(block in arb_block(), params in arb_pulse_params()) {
        let mut p = PulseTrain::default();
        p.set_sample_rate(48_000.0);
        p.set_params(&params);
        let mut out = vec![Frame::default(); block.len()];
        p.process_block(&block, &mut out);
        for f in &out {
            prop_assert!(f[0].is_finite(), "left not finite: {}", f[0]);
            prop_assert!(f[1].is_finite(), "right not finite: {}", f[1]);
        }
    }

    #[test]
    fn pulse_output_bounded(block in arb_block(), params in arb_pulse_params()) {
        let mut p = PulseTrain::default();
        p.set_sample_rate(48_000.0);
        p.set_params(&params);
        let mut out = vec![Frame::default(); block.len()];
        p.process_block(&block, &mut out);
        let ceiling = libm::pow(10.0, f64::from(params.level_dbfs) / 20.0) as f32 * (1.0 + EPSILON);
        for f in &out {
            prop_assert!(f[0].abs() <= 1.0 + EPSILON, "left unbounded: {}", f[0]);
            prop_assert!(f[1].abs() <= 1.0 + EPSILON, "right unbounded: {}", f[1]);
            prop_assert!(f[0].abs() <= ceiling, "left exceeds {:?}: {}", ceiling, f[0]);
            prop_assert!(f[1].abs() <= ceiling, "right exceeds {:?}: {}", ceiling, f[1]);
        }
    }

    #[test]
    fn pulse_reset_idempotent(block in arb_block(), params in arb_pulse_params()) {
        let mut pa = PulseTrain::default();
        let mut pb = PulseTrain::default();
        pa.set_sample_rate(48_000.0);
        pb.set_sample_rate(48_000.0);
        pa.set_params(&params);
        pb.set_params(&params);

        let junk: Vec<Frame> = (0..64).map(|i| [(i as f32 / 32.0 - 1.0), -(i as f32 / 32.0 - 1.0)]).collect();
        let mut jo = vec![Frame::default(); junk.len()];
        pa.process_block(&junk, &mut jo);
        pb.process_block(&junk, &mut jo);

        pa.reset();
        pb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        pa.process_block(&block, &mut oa);
        pb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn pulse_block_equals_tick(block in arb_block(), params in arb_pulse_params()) {
        let n = block.len();
        if n == 0 { return Ok(()); }

        let mut fb = PulseTrain::default();
        fb.set_sample_rate(48_000.0);
        fb.set_params(&params);
        let mut ob = vec![Frame::default(); n];
        fb.process_block(&block, &mut ob);

        let mut ft = PulseTrain::default();
        ft.set_sample_rate(48_000.0);
        ft.set_params(&params);
        let ot: Vec<Frame> = block.iter().map(|f| ft.tick(*f)).collect();

        for (i, (a, b)) in ob.iter().zip(ot.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }

    #[test]
    fn pulse_bit_identical_determinism(block in arb_block(), params in arb_pulse_params()) {
        let mut pa = PulseTrain::default();
        pa.set_sample_rate(48_000.0);
        pa.set_params(&params);
        let mut oa = vec![Frame::default(); block.len()];
        pa.process_block(&block, &mut oa);

        let mut pb = PulseTrain::default();
        pb.set_sample_rate(48_000.0);
        pb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        pb.process_block(&block, &mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }
}
