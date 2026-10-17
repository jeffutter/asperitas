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
// Processor laws: one harness, instantiated per effect
// ---------------------------------------------------------------------------

/// Sample rate every law runs at.
const RATE: f32 = 48_000.0;

/// Longest smoothing time constant among the processors, in samples at `RATE` (Gain: 10 ms).
const SLOWEST_SMOOTHING_SAMPLES: usize = 480;

fn configured<P: Processor + Default>(params: &P::Params) -> P {
    let mut p = P::default();
    p.set_sample_rate(RATE);
    p.set_params(params);
    p
}

fn render_block<P: Processor>(p: &mut P, block: &[Frame]) -> Vec<Frame> {
    let mut out = block.to_vec();
    p.process_block(&mut out);
    out
}

fn peak_of(block: &[Frame]) -> f32 {
    block
        .iter()
        .flat_map(|f| f.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()))
}

fn assert_same(a: &[Frame], b: &[Frame], tol: f32) -> Result<(), TestCaseError> {
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        prop_assert!(
            (x[0] - y[0]).abs() <= tol && (x[1] - y[1]).abs() <= tol,
            "mismatch at sample {}: {:?} != {:?}",
            i,
            x,
            y
        );
    }
    Ok(())
}

/// Finite output for any in-range-or-nearby params, and never louder than `bound` allows.
fn law_finite_and_bounded<P: Processor + Default>(
    params: &P::Params,
    block: &[Frame],
    bound: impl Fn(&P::Params, f32) -> f32,
) -> Result<(), TestCaseError> {
    let out = render_block(&mut configured::<P>(params), block);
    let limit = bound(params, peak_of(block)) * (1.0 + EPSILON) + EPSILON;
    for f in &out {
        prop_assert!(f[0].is_finite() && f[1].is_finite(), "not finite: {:?}", f);
        prop_assert!(
            f[0].abs() <= limit && f[1].abs() <= limit,
            "{:?} exceeds bound {}",
            f,
            limit
        );
    }
    Ok(())
}

/// After any signal, silence in settles to silence out.
fn law_silence<P: Processor + Default>(
    params: &P::Params,
    history: &[Frame],
) -> Result<(), TestCaseError> {
    let mut p = configured::<P>(params);
    render_block(&mut p, history);
    // The slowest tail is a 20 Hz one-pole (about 380 samples per time constant); 20k samples is
    // over 50 of them.
    let out = render_block(&mut p, &vec![Frame::default(); 20_000]);
    for f in &out[10_000..] {
        prop_assert!(
            f[0].abs() < 1e-6 && f[1].abs() < 1e-6,
            "not silent: {:?}",
            f
        );
    }
    Ok(())
}

/// `process_block` and a `tick` loop are the same function.
fn law_block_equals_tick<P: Processor + Default>(
    params: &P::Params,
    block: &[Frame],
) -> Result<(), TestCaseError> {
    let by_block = render_block(&mut configured::<P>(params), block);
    let mut t = configured::<P>(params);
    let by_tick: Vec<Frame> = block.iter().map(|f| t.tick(*f)).collect();
    assert_same(&by_block, &by_tick, EPSILON)
}

/// The `Processor::reset` contract: after reset, identical input gives identical output whatever
/// ran before. Two instances take different histories under different params, are brought to
/// the same params, reset, and must then agree bit for bit.
fn law_reset_forgets_history<P: Processor + Default>(
    params: &P::Params,
    other_params: &P::Params,
    history_a: &[Frame],
    history_b: &[Frame],
    probe: &[Frame],
) -> Result<(), TestCaseError> {
    let mut a = configured::<P>(other_params);
    render_block(&mut a, history_a);
    let mut b = configured::<P>(params);
    render_block(&mut b, history_b);
    for p in [&mut a, &mut b] {
        p.set_params(params);
        p.reset();
    }
    let (oa, ob) = (render_block(&mut a, probe), render_block(&mut b, probe));
    for (i, (x, y)) in oa.iter().zip(&ob).enumerate() {
        prop_assert!(
            x[0].to_bits() == y[0].to_bits() && x[1].to_bits() == y[1].to_bits(),
            "histories leaked through reset at sample {}: {:?} != {:?}",
            i,
            x,
            y
        );
    }
    Ok(())
}

/// A parameter step must not click. Over the whole block after the step, the largest sample to
/// sample change may exceed neither what the signal does under either setting held steady (with
/// a little headroom for a transition passing between them) nor a small fraction of the largest
/// instantaneous gap between the two steady outputs. An unsmoothed step puts that whole gap into
/// one sample; the smoothers' time constants are at least 240 samples, so the fraction is
/// generous.
fn law_param_step_is_smooth<P: Processor + Default>(
    a: &P::Params,
    b: &P::Params,
) -> Result<(), TestCaseError> {
    const SETTLE: usize = 8 * SLOWEST_SMOOTHING_SAMPLES;
    const WINDOW: usize = 4 * SLOWEST_SMOOTHING_SAMPLES;
    let sine: Vec<Frame> = (0..SETTLE + WINDOW)
        .map(|n| {
            let v = 0.5 * libm::sinf(2.0 * core::f32::consts::PI * 997.0 * n as f32 / RATE);
            [v, -v]
        })
        .collect();
    let max_delta = |out: &[Frame]| {
        out.windows(2)
            .flat_map(|w| [(w[1][0] - w[0][0]).abs(), (w[1][1] - w[0][1]).abs()])
            .fold(0.0f32, f32::max)
    };
    let steady = |params: &P::Params| {
        let mut p = configured::<P>(params);
        p.reset();
        render_block(&mut p, &sine)[SETTLE - 1..].to_vec()
    };
    let (ya, yb) = (steady(a), steady(b));

    let mut p = configured::<P>(a);
    p.reset();
    let mut before = render_block(&mut p, &sine[..SETTLE]);
    p.set_params(b);
    let after = render_block(&mut p, &sine[SETTLE..]);
    before.drain(..SETTLE - 1);
    before.extend(after);

    let gap = ya
        .iter()
        .zip(&yb)
        .flat_map(|(x, y)| [(x[0] - y[0]).abs(), (x[1] - y[1]).abs()])
        .fold(0.0f32, f32::max);
    let allowed = 1.1 * max_delta(&ya).max(max_delta(&yb)) + gap / 20.0 + EPSILON;
    prop_assert!(
        max_delta(&before) <= allowed,
        "step clicked: max delta {} > {} (gap {})",
        max_delta(&before),
        allowed,
        gap
    );
    Ok(())
}

/// Hostile params and non-finite or huge input never produce NaN, and the processor recovers to
/// finite output once sane params return.
fn law_survives_hostile_input<P: Processor + Default>(
    hostile: &P::Params,
    bad: Frame,
    block: &[Frame],
) -> Result<(), TestCaseError> {
    let mut p = configured::<P>(hostile);
    for _ in 0..64 {
        // A huge finite input times the gain ceiling may overflow to infinity, as the output is
        // unclamped; a NaN would mean bad input leaked into arithmetic.
        let f = p.tick(bad);
        prop_assert!(!f[0].is_nan() && !f[1].is_nan(), "NaN out: {:?}", f);
    }
    p.set_params(&P::Params::default());
    let out = render_block(&mut p, block);
    for f in &out {
        prop_assert!(f[0].is_finite() && f[1].is_finite(), "no recovery: {:?}", f);
    }
    Ok(())
}

/// A frame that is a number, an infinity, a NaN or just huge.
fn arb_hostile_frame() -> impl Strategy<Value = Frame> {
    let v = prop_oneof![
        any::<f32>(),
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
        Just(f32::MAX),
    ];
    (v.clone(), v).prop_map(|(l, r)| [l, r])
}

/// Instantiate every law for processor `$ty`, as module `$m`.
///
/// - `params`: strategy for params in and a little beyond the knob range.
/// - `hostile`: strategy for params that include NaN, infinities and extremes.
/// - `bound`: `|params, input_peak| -> f32`, the largest output magnitude the processor may make.
macro_rules! processor_laws {
    ($m:ident, $ty:ty, params: $params:expr, hostile: $hostile:expr, bound: $bound:expr $(,)?) => {
        mod $m {
            use super::*;

            proptest! {
                #[test]
                fn finite_and_bounded(params in $params, block in arb_block()) {
                    law_finite_and_bounded::<$ty>(&params, &block, $bound)?;
                }

                #[test]
                fn silence_in_silence_out(params in $params, history in arb_block()) {
                    law_silence::<$ty>(&params, &history)?;
                }

                #[test]
                fn block_equals_tick(params in $params, block in arb_block()) {
                    law_block_equals_tick::<$ty>(&params, &block)?;
                }

                #[test]
                fn reset_forgets_history(
                    params in $params,
                    other_params in $params,
                    history_a in arb_block(),
                    history_b in arb_block(),
                    probe in arb_block(),
                ) {
                    law_reset_forgets_history::<$ty>(
                        &params, &other_params, &history_a, &history_b, &probe,
                    )?;
                }

                #[test]
                fn param_step_is_smooth(a in $params, b in $params) {
                    law_param_step_is_smooth::<$ty>(&a, &b)?;
                }

                #[test]
                fn survives_hostile_input(
                    hostile in $hostile,
                    bad in arb_hostile_frame(),
                    block in arb_block(),
                ) {
                    law_survives_hostile_input::<$ty>(&hostile, bad, &block)?;
                }
            }
        }
    };
}

/// An f32 that is arbitrary, NaN, infinite or extreme.
fn arb_hostile_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        any::<f32>(),
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
        Just(0.0),
        Just(f32::MAX),
        Just(f32::MIN),
    ]
}

processor_laws!(
    gain_laws,
    Gain,
    params: (GAIN_PARAMS[0].min - 20.0..=GAIN_PARAMS[0].max + 10.0)
        .prop_map(|gain_db| GainParams { gain_db }),
    hostile: arb_hostile_f32().prop_map(|gain_db| GainParams { gain_db }),
    // The smoother ramps from unity to the target, so the gain never exceeds the larger of
    // the two; the linear gain is clamped at the ceiling `Gain` applies.
    bound: |p: &GainParams, peak: f32| {
        peak * libm::powf(10.0, p.gain_db / 20.0).clamp(0.0, 8192.0).max(1.0)
    },
);

processor_laws!(
    filter_laws,
    OnePoleLowPass,
    // 0 Hz is left out: it freezes the filter state, so it is a hostile value, not a silent one.
    params: (FILTER_PARAMS[0].min..=FILTER_PARAMS[0].max * 1.5)
        .prop_map(|cutoff_hz| FilterParams { cutoff_hz }),
    hostile: arb_hostile_f32().prop_map(|cutoff_hz| FilterParams { cutoff_hz }),
    // A one-pole low-pass is a convex average of past inputs, so it cannot exceed the largest
    // input magnitude.
    bound: |_: &FilterParams, peak: f32| peak,
);

proptest! {
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
        let oa = render_block(&mut fa, &block);
        let ob = render_block(&mut fb, &block);
        for (a, b) in oa.iter().zip(&ob) {
            prop_assert_eq!(a[0].to_bits(), b[0].to_bits());
            prop_assert_eq!(a[1].to_bits(), b[1].to_bits());
        }
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

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
