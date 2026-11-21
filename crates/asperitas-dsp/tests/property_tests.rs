//! Property-based tests for any `Processor` implementation.
//!
//! Run with: `cargo test -p asperitas-dsp`

use asperitas_dsp::{
    EnvelopeFollower, EnvelopeParams, Excitation, ExcitationParams, FilterParams, Frame, Gain,
    GainParams, OnePoleLowPass, Processor, RandomWalk, RandomWalkParams, Resonator,
    ResonatorParams, ResonatorVoice, ResonatorVoiceParams, BANK_GAIN, ENVELOPE_PARAMS,
    EXCITATION_PARAMS, FILTER_PARAMS, GAIN_PARAMS, RANDOM_WALK_PARAMS, RESONATOR_PARAMS,
    RESONATOR_VOICE_PARAMS, VOICE_COUNT,
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
    tail: impl Fn(&P::Params) -> usize,
) -> Result<(), TestCaseError> {
    let mut p = configured::<P>(params);
    render_block(&mut p, history);
    // Wait `tail` samples, then require silence over the next `tail`.
    let tail = tail(params);
    let out = render_block(&mut p, &vec![Frame::default(); 2 * tail]);
    for f in &out[tail..] {
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
/// one sample; the smoothers' time constants are at least 240 samples, so a tenth of it is
/// generous. The fraction is not smaller because a transition can legitimately be steeper than
/// either steady state: sweeping a high-pass cut away from a tone makes the level rise, which the
/// excitation stage reads as an attack and emphasises, so the transition's slope passes about
/// 40 percent above the steeper steady one.
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
    let allowed = 1.1 * max_delta(&ya).max(max_delta(&yb)) + gap / 10.0 + EPSILON;
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
/// - `tail`: `|params| -> usize`, samples after which a decayed tail must be silent.
macro_rules! processor_laws {
    ($m:ident, $ty:ty, params: $params:expr, hostile: $hostile:expr, bound: $bound:expr, tail: $tail:expr $(,)?) => {
        mod $m {
            use super::*;

            proptest! {
                #[test]
                fn finite_and_bounded(params in $params, block in arb_block()) {
                    law_finite_and_bounded::<$ty>(&params, &block, $bound)?;
                }

                #[test]
                fn silence_in_silence_out(params in $params, history in arb_block()) {
                    law_silence::<$ty>(&params, &history, $tail)?;
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
    tail: |_: &GainParams| 10_000,
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
    // The slowest tail is a 20 Hz one-pole (about 380 samples per time constant); 10k samples is
    // over 25 of them.
    tail: |_: &FilterParams| 10_000,
);

processor_laws!(
    resonator_laws,
    ResonatorVoice,
    // Decay 0 and pitch above Nyquist are included: they are sanitised, not hostile.
    params: (
        RESONATOR_VOICE_PARAMS[0].min..=RESONATOR_VOICE_PARAMS[0].max * 1.5,
        0.0f32..=RESONATOR_VOICE_PARAMS[1].max * 1.5,
    )
        .prop_map(|(pitch_hz, decay_s)| ResonatorVoiceParams { pitch_hz, decay_s }),
    hostile: (arb_hostile_f32(), arb_hostile_f32())
        .prop_map(|(pitch_hz, decay_s)| ResonatorVoiceParams { pitch_hz, decay_s }),
    // The state is a contraction by r < 1 plus (1 - r) times the input, so it never exceeds the
    // input peak.
    bound: |_: &ResonatorVoiceParams, peak: f32| peak,
    // Silent after two decay times (-120 dB), with margin. A decay under the 1 ms floor is held
    // at the floor.
    tail: |p: &ResonatorVoiceParams| {
        (2.5 * p.decay_s.max(0.001) * RATE) as usize + 10_000
    },
);

processor_laws!(
    excitation_laws,
    Excitation,
    params: (
        EXCITATION_PARAMS[0].min - 5.0..=EXCITATION_PARAMS[0].max * 1.5,
        -0.2f32..=1.2,
        EXCITATION_PARAMS[2].min - 0.1..=EXCITATION_PARAMS[2].max * 1.5,
    )
        .prop_map(|(low_cut_hz, transient, drive_limit)| ExcitationParams {
            low_cut_hz,
            transient,
            drive_limit,
        }),
    hostile: (arb_hostile_f32(), arb_hostile_f32(), arb_hostile_f32()).prop_map(
        |(low_cut_hz, transient, drive_limit)| ExcitationParams {
            low_cut_hz,
            transient,
            drive_limit,
        }
    ),
    // The limiter is last, so output stays under its ceiling. The ceiling starts at the default
    // (the maximum of its range) and glides to a target clamped into the range, so it never
    // exceeds the default ceiling. Independently, the high-pass output is at most twice the
    // input peak (input minus a convex average of past inputs) and the transient stage at most
    // triples it, and the limiter never raises a magnitude, so 6 x peak also bounds the output.
    bound: |_: &ExcitationParams, peak: f32| (6.0 * peak).min(EXCITATION_PARAMS[2].max),
    // The slowest tail is the 10 Hz high-pass state (about 764 samples per time constant); 20k
    // samples is over 26 of them.
    tail: |_: &ExcitationParams| 20_000,
);

/// Bank params in and a little beyond the table range, the root past the pitch ceiling included.
fn arb_bank_params() -> impl Strategy<Value = ResonatorParams> {
    (
        RESONATOR_PARAMS[0].min - 5.0..=RESONATOR_PARAMS[0].max * 4.0,
        -0.2f32..=1.2,
        -0.2f32..=1.2,
        -0.2f32..=1.2,
        (
            EXCITATION_PARAMS[0].min - 5.0..=EXCITATION_PARAMS[0].max * 1.5,
            -0.2f32..=1.2,
            EXCITATION_PARAMS[2].min - 0.1..=EXCITATION_PARAMS[2].max * 1.5,
        ),
    )
        .prop_map(
            |(root_hz, structure, damping, spread, (low_cut_hz, transient, drive_limit))| {
                ResonatorParams {
                    root_hz,
                    structure,
                    damping,
                    spread,
                    excitation: ExcitationParams {
                        low_cut_hz,
                        transient,
                        drive_limit,
                    },
                }
            },
        )
}

/// Bank params that are NaN, infinite or extreme in every field.
fn arb_hostile_bank_params() -> impl Strategy<Value = ResonatorParams> {
    (
        arb_hostile_f32(),
        arb_hostile_f32(),
        arb_hostile_f32(),
        arb_hostile_f32(),
        (arb_hostile_f32(), arb_hostile_f32(), arb_hostile_f32()),
    )
        .prop_map(
            |(root_hz, structure, damping, spread, (low_cut_hz, transient, drive_limit))| {
                ResonatorParams {
                    root_hz,
                    structure,
                    damping,
                    spread,
                    excitation: ExcitationParams {
                        low_cut_hz,
                        transient,
                        drive_limit,
                    },
                }
            },
        )
}

/// The documented bank bound: every voice is bounded by its input, an equal-power gain is at most
/// one, and the excitation output is at most `min(drive_limit_max, 6 * peak)`.
fn bank_bound(peak: f32) -> f32 {
    BANK_GAIN * VOICE_COUNT as f32 * (6.0 * peak).min(EXCITATION_PARAMS[2].max)
}

processor_laws!(
    resonator_bank_laws,
    Resonator,
    params: arb_bank_params(),
    hostile: arb_hostile_bank_params(),
    bound: |_: &ResonatorParams, peak: f32| bank_bound(peak),
    // Silent after two and a half root decays (the root rings longest) plus the excitation
    // high-pass settling, with margin.
    tail: |p: &ResonatorParams| (2.5 * p.root_decay_s() * RATE) as usize + 20_000,
);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The worst case: longest decay and heaviest drive at either end of structure and across the
    /// root range, with full-scale and hostile input, stays finite and under the bound.
    #[test]
    fn bank_worst_case_stays_under_the_bound(
        root_hz in RESONATOR_PARAMS[0].min..=RESONATOR_PARAMS[0].max,
        structure in prop_oneof![Just(0.0f32), Just(1.0f32)],
        spread in prop_oneof![Just(0.0f32), Just(1.0f32)],
        sign in prop_oneof![Just(1.0f32), Just(-1.0f32)],
        phase in 0usize..64,
        hostile in prop::bool::ANY,
    ) {
        let params = ResonatorParams {
            root_hz,
            structure,
            damping: RESONATOR_PARAMS[2].min,
            spread,
            excitation: ExcitationParams {
                transient: 1.0,
                ..ExcitationParams::default()
            },
        };
        let mut r = Resonator::new(RATE, &params);
        let limit = bank_bound(1.0) * (1.0 + EPSILON);
        for n in 0..20_000usize {
            // Full-scale square wave, sometimes with a non-finite sample mixed in.
            let v = if (n + phase) / 16 % 2 == 0 { sign } else { -sign };
            let v = if hostile && n % 97 == 0 { f32::NAN } else { v };
            let f = r.tick([v, v]);
            prop_assert!(f[0].is_finite() && f[1].is_finite());
            prop_assert!(f[0].abs() <= limit && f[1].abs() <= limit, "{:?} > {}", f, limit);
        }
    }
}

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

// ---------------------------------------------------------------------------
// Envelope follower (a mono control source, not a Processor)
// ---------------------------------------------------------------------------

fn arb_envelope_params() -> impl Strategy<Value = EnvelopeParams> {
    // In range and well beyond it, plus non-finite values.
    let ms = |spec: &'static asperitas_dsp::ParamSpec| {
        prop_oneof![
            spec.min..=spec.max,
            (spec.min * 0.01)..=(spec.max * 10.0),
            arb_hostile_f32(),
        ]
    };
    (ms(&ENVELOPE_PARAMS[0]), ms(&ENVELOPE_PARAMS[1])).prop_map(|(attack_ms, release_ms)| {
        EnvelopeParams {
            attack_ms,
            release_ms,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn envelope_is_finite_non_negative_and_bounded_by_the_input_peak(
        params in arb_envelope_params(),
        input in prop::collection::vec(arb_hostile_f32(), 1..600),
    ) {
        let mut f = EnvelopeFollower::new(RATE, &params);
        let mut peak = 0.0f32;
        for x in input {
            if x.is_finite() { peak = peak.max(x.abs()); }
            let y = f.tick(x);
            prop_assert!(y.is_finite() && y >= 0.0, "y = {}", y);
            // One part in a million of rounding slack: the update is a convex step in f32.
            prop_assert!(y <= peak * (1.0 + 1e-6), "y = {} peak = {}", y, peak);
        }
    }

    #[test]
    fn envelope_block_equals_ticks_and_reset_is_idempotent(
        params in arb_envelope_params(),
        input in prop::collection::vec(arb_hostile_f32(), 1..400),
    ) {
        let mut a = EnvelopeFollower::new(RATE, &params);
        let mut b = a.clone();
        let mut out = vec![0.0f32; input.len()];
        a.process_block(&input, &mut out);
        for (x, o) in input.iter().zip(&out) {
            prop_assert_eq!(b.tick(*x).to_bits(), o.to_bits());
        }
        a.reset();
        let once = a.clone();
        a.reset();
        prop_assert_eq!(a.value().to_bits(), once.value().to_bits());
        prop_assert_eq!(a.value(), 0.0);
    }
}

// ---------------------------------------------------------------------------
// Random walk (a mono control source, not a Processor)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_walk_is_finite_in_range_and_step_bounded(
        seed in any::<u32>(),
        rate in prop_oneof![
            RANDOM_WALK_PARAMS[0].min..=RANDOM_WALK_PARAMS[0].max,
            Just(RANDOM_WALK_PARAMS[0].max),
            arb_hostile_f32(),
        ],
        fs in prop_oneof![Just(44_100.0f32), Just(48_000.0f32), 8_000.0f32..=192_000.0],
    ) {
        let mut w = RandomWalk::new(fs, seed, &RandomWalkParams { rate_hz: rate });
        let bound = w.max_step();
        let mut prev = w.value();
        for _ in 0..4000 {
            let y = w.tick();
            prop_assert!(y.is_finite() && (-1.0..=1.0).contains(&y), "y = {}", y);
            // Slack for f32 rounding in the interpolation.
            prop_assert!((y - prev).abs() <= bound * 1.0001 + 1e-6, "step {} bound {}", (y - prev).abs(), bound);
            prev = y;
        }
    }

    #[test]
    fn random_walk_block_equals_ticks_for_any_split(
        seed in any::<u32>(),
        rate in RANDOM_WALK_PARAMS[0].min..=RANDOM_WALK_PARAMS[0].max,
        splits in prop::collection::vec(1usize..700, 1..8),
    ) {
        let mut a = RandomWalk::new(RATE, seed, &RandomWalkParams { rate_hz: rate });
        let mut b = a.clone();
        for len in splits {
            let mut out = vec![0.0f32; len];
            a.process_block(&mut out);
            for o in &out {
                prop_assert_eq!(b.tick().to_bits(), o.to_bits());
            }
        }
        a.reset();
        let mut fresh = RandomWalk::new(RATE, seed, &RandomWalkParams { rate_hz: rate });
        // Same rate in force, so reset replays the start of the sequence.
        for _ in 0..300 {
            prop_assert_eq!(a.tick().to_bits(), fresh.tick().to_bits());
        }
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
