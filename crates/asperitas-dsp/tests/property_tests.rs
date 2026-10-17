//! Property-based tests for any `Processor` implementation.
//!
//! Run with: `cargo test -p asperitas-dsp`

use asperitas_dsp::{
    ExponentialSweep, ExponentialSweepParams, FilterParams, Frame, Gain, GainParams,
    OnePoleLowPass, Processor, PulseTrain, PulseTrainParams, Sine, SineParams, FILTER_PARAMS,
    GAIN_PARAMS,
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
        out.clone_from(&block);
        g.process_block(&mut out);
        for f in &out {
            prop_assert!(f[0].is_finite(), "left not finite: {}", f[0]);
            prop_assert!(f[1].is_finite(), "right not finite: {}", f[1]);
        }
    }

    #[test]
    fn gain_output_bounded(block in arb_block(), gain_db in GAIN_PARAMS[0].min..=GAIN_PARAMS[0].max) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams { gain_db });
        let mut out = vec![Frame::default(); block.len()];
        out.clone_from(&block);
        g.process_block(&mut out);
        // The smoother ramps from unity to the target, so the gain never exceeds the larger of
        // the two. Output is unclamped, so this bound is real.
        let max_gain = libm::powf(10.0, gain_db / 20.0).max(1.0);
        for (f, i) in out.iter().zip(&block) {
            prop_assert!(f[0].abs() <= i[0].abs() * max_gain * (1.0 + EPSILON) + EPSILON, "left unbounded: {}", f[0]);
            prop_assert!(f[1].abs() <= i[1].abs() * max_gain * (1.0 + EPSILON) + EPSILON, "right unbounded: {}", f[1]);
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
        wo.clone_from(&warmup);
        g.process_block(&mut wo);

        // Feed silence
        let silence = vec![Frame::default(); block_size.max(512)];
        let mut out = vec![Frame::default(); silence.len()];
        out.clone_from(&silence);
        g.process_block(&mut out);

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
        jo.clone_from(&junk);
        ga.process_block(&mut jo);
        jo.clone_from(&junk);
        gb.process_block(&mut jo);

        ga.reset();
        gb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        ga.process_block(&mut oa);
        ob.clone_from(&block);
        gb.process_block(&mut ob);

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
        ob.clone_from(&block);
        gb.process_block(&mut ob);

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
    fn gain_param_change_smooth(gain_db_a in GAIN_PARAMS[0].min..=GAIN_PARAMS[0].max, gain_db_b in GAIN_PARAMS[0].min..=GAIN_PARAMS[0].max) {
        let mut g = Gain::default();
        g.set_sample_rate(48_000.0);
        g.set_params(&GainParams { gain_db: gain_db_a });

        // Warm up so smoothing converges
        let warmup = vec![Frame::from([0.5, 0.5]); 256];
        let mut wo = vec![Frame::default(); warmup.len()];
        wo.clone_from(&warmup);
        g.process_block(&mut wo);

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
        out.clone_from(&block);
        f.process_block(&mut out);
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
        out.clone_from(&block);
        f.process_block(&mut out);
        // A one-pole low-pass is a convex average of past inputs, so it cannot exceed the
        // largest input magnitude. Output is unclamped, so a runaway would show here.
        let peak = block.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, v| m.max(v.abs()));
        for frame in &out {
            prop_assert!(frame[0].abs() <= peak + EPSILON, "left unbounded: {}", frame[0]);
            prop_assert!(frame[1].abs() <= peak + EPSILON, "right unbounded: {}", frame[1]);
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
        wo.clone_from(&warmup);
        f.process_block(&mut wo);

        // Feed extended silence.
        // At default 20 kHz cutoff with 48 kHz sample rate, alpha ≈ 0.999,
        // so the time constant is ~7500 samples. We need ~5× that to decay.
        let silence_len = 50_000;
        let silence = vec![Frame::default(); silence_len];
        let mut out = vec![Frame::default(); silence_len];
        out.clone_from(&silence);
        f.process_block(&mut out);

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
    fn filter_rate_and_params_order_is_irrelevant(
        cutoff in 0.0f32..FILTER_PARAMS[0].max * 1.5,
        rate in 4_000.0f32..96_000.0,
        block in arb_block(),
    ) {
        let params = FilterParams { cutoff_hz: cutoff };
        let mut fa = OnePoleLowPass::default();
        fa.set_params(&params);
        fa.set_sample_rate(rate);
        let mut fb = OnePoleLowPass::default();
        fb.set_sample_rate(rate);
        fb.set_params(&params);
        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        fa.process_block(&mut oa);
        ob.clone_from(&block);
        fb.process_block(&mut ob);
        for (a, b) in oa.iter().zip(&ob) {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits());
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits());
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
        jo.clone_from(&junk);
        fa.process_block(&mut jo);
        jo.clone_from(&junk);
        fb.process_block(&mut jo);

        fa.reset();
        fb.reset();

        let mut oa = vec![Frame::default(); block.len()];
        let mut ob = vec![Frame::default(); block.len()];
        oa.clone_from(&block);
        fa.process_block(&mut oa);
        ob.clone_from(&block);
        fb.process_block(&mut ob);

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
        ob.clone_from(&block);
        fb.process_block(&mut ob);

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
    fn filter_param_change_smooth(cutoff_a in FILTER_PARAMS[0].min..=FILTER_PARAMS[0].max, cutoff_b in FILTER_PARAMS[0].min..=FILTER_PARAMS[0].max) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams { cutoff_hz: cutoff_a });

        // Warm up so smoothing converges
        let warmup = vec![Frame::from([0.5, 0.5]); 256];
        let mut wo = vec![Frame::default(); warmup.len()];
        wo.clone_from(&warmup);
        f.process_block(&mut wo);

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

// ---------------------------------------------------------------------------
// Non-finite robustness and exact-zero decay
// ---------------------------------------------------------------------------

fn run_finite_after_event<P: Processor>(
    p: &mut P,
    bad: &P::Params,
    good: &P::Params,
    bad_input: Frame,
) -> bool {
    p.set_sample_rate(48_000.0);
    p.set_params(bad);
    for _ in 0..64 {
        p.tick(bad_input);
    }
    p.set_params(good);
    let block = vec![[0.5f32, -0.5]; 512];
    let mut out = vec![Frame::default(); block.len()];
    out.clone_from(&block);
    p.process_block(&mut out);
    out.iter().all(|f| f[0].is_finite() && f[1].is_finite())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn gain_recovers_after_any_param_or_input(
        db in any::<f32>(), x in any::<f32>(), y in any::<f32>()
    ) {
        let mut g = Gain::default();
        assert!(run_finite_after_event(
            &mut g,
            &GainParams { gain_db: db },
            &GainParams::default(),
            [x, y],
        ));
    }

    #[test]
    fn filter_recovers_after_any_param_or_input(
        fc in any::<f32>(), x in any::<f32>(), y in any::<f32>()
    ) {
        let mut f = OnePoleLowPass::default();
        assert!(run_finite_after_event(
            &mut f,
            &FilterParams { cutoff_hz: fc },
            &FilterParams::default(),
            [x, y],
        ));
    }

    #[test]
    fn filter_state_reaches_exactly_zero_in_silence(
        fc in FILTER_PARAMS[0].min..FILTER_PARAMS[0].max, level in -1.0f32..=1.0
    ) {
        let mut f = OnePoleLowPass::default();
        f.set_sample_rate(48_000.0);
        f.set_params(&FilterParams { cutoff_hz: fc });
        f.reset();
        for _ in 0..1000 { f.tick([level, level]); }
        // Slowest case: 20 Hz cutoff; 1e-30 needs about 69 time constants (~26k samples at
        // alpha ~ 0.0026). 300k samples is ample.
        let mut last = [1.0f32; 2];
        for _ in 0..300_000 { last = f.tick([0.0, 0.0]); }
        prop_assert_eq!(last, [0.0, 0.0]);
    }
}

#[test]
fn gain_and_filter_survive_nan_deterministically() {
    let mut g = Gain::default();
    assert!(run_finite_after_event(
        &mut g,
        &GainParams { gain_db: f32::NAN },
        &GainParams::default(),
        [f32::NAN, f32::INFINITY],
    ));
    let mut f = OnePoleLowPass::default();
    assert!(run_finite_after_event(
        &mut f,
        &FilterParams {
            cutoff_hz: f32::NAN
        },
        &FilterParams::default(),
        [f32::NAN, f32::NEG_INFINITY],
    ));
}

#[test]
fn gain_smoother_tail_reaches_exactly_zero() {
    let mut g = Gain::default();
    g.set_sample_rate(48_000.0);
    g.set_params(&GainParams { gain_db: -1000.0 });
    let mut last = [1.0f32; 2];
    for _ in 0..200_000 {
        last = g.tick([1.0, 1.0]);
    }
    assert_eq!(last, [0.0, 0.0]);
}
