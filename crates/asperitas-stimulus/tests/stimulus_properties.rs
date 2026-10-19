//! Property-based tests for the stimulus generators.
//!
//! Run with: `cargo test -p asperitas-stimulus --test stimulus_properties`

use asperitas_dsp::{Frame, Processor};
use asperitas_stimulus::{
    ExponentialSweep, ExponentialSweepParams, PulseTrain, PulseTrainParams, Sine, SineParams,
};
use proptest::prelude::*;

/// A small positive epsilon for float comparisons.
const EPSILON: f32 = 1e-6;

/// Generate a block of frames (power-of-2 lengths matching typical DMA sizes).
fn arb_block() -> impl Strategy<Value = Vec<Frame>> {
    let frame = (-1.0f32..=1.0).prop_flat_map(|l| (-1.0f32..=1.0).prop_map(move |r| [l, r]));
    prop_oneof![
        prop::collection::vec(frame.clone(), 4..64),
        prop::collection::vec(frame.clone(), 64..256),
        prop::collection::vec(frame.clone(), 256..512),
    ]
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
        out.clone_from(&block);
        s.process_block(&mut out);
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
        out.clone_from(&block);
        s.process_block(&mut out);
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
        jo.clone_from(&junk);
        sa.process_block(&mut jo);
        jo.clone_from(&junk);
        sb.process_block(&mut jo);

        sa.reset();
        sb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        sa.process_block(&mut oa);
        ob.clone_from(&block);
        sb.process_block(&mut ob);

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
        ob.clone_from(&block);
        fb.process_block(&mut ob);

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
        oa.clone_from(&block);
        sa.process_block(&mut oa);

        // Built the same way but later, with no shared state beyond the parameter struct.
        let mut sb = Sine::default();
        sb.set_sample_rate(48_000.0);
        sb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        ob.clone_from(&block);
        sb.process_block(&mut ob);

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
        out.clone_from(&block);
        e.process_block(&mut out);
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
        out.clone_from(&block);
        e.process_block(&mut out);
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
        jo.clone_from(&junk);
        ea.process_block(&mut jo);
        jo.clone_from(&junk);
        eb.process_block(&mut jo);

        ea.reset();
        eb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        ea.process_block(&mut oa);
        ob.clone_from(&block);
        eb.process_block(&mut ob);

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
        ob.clone_from(&block);
        fb.process_block(&mut ob);

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
        oa.clone_from(&block);
        ea.process_block(&mut oa);

        let mut eb = ExponentialSweep::default();
        eb.set_sample_rate(48_000.0);
        eb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        ob.clone_from(&block);
        eb.process_block(&mut ob);

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
        out.clone_from(&block);
        p.process_block(&mut out);
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
        out.clone_from(&block);
        p.process_block(&mut out);
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
        jo.clone_from(&junk);
        pa.process_block(&mut jo);
        jo.clone_from(&junk);
        pb.process_block(&mut jo);

        pa.reset();
        pb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        pa.process_block(&mut oa);
        ob.clone_from(&block);
        pb.process_block(&mut ob);

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
        ob.clone_from(&block);
        fb.process_block(&mut ob);

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
        oa.clone_from(&block);
        pa.process_block(&mut oa);

        let mut pb = PulseTrain::default();
        pb.set_sample_rate(48_000.0);
        pb.set_params(&params);
        let mut ob = vec![Frame::default(); block.len()];
        ob.clone_from(&block);
        pb.process_block(&mut ob);

        for (i, (a, b)) in oa.iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits(), "left mismatch at {}", i);
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits(), "right mismatch at {}", i);
        }
    }
}
