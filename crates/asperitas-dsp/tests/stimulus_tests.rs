//! Waveform-shape tests for the stimulus generators.
//!
//! These sit beside [`property_tests`](../property_tests) rather than inside it because they
//! answer the opposite question. proptest asks "does *some* input break this"; these ask "for the
//! parameters a capture will actually use, is this the waveform the analysis intends to subtract".
//! That claim is about particular numbers — a peak of 0.1, a pulse spread over three samples,
//! nothing above 8 kHz — which read as arbitrary constants inside a property harness.
//!
//! Two house properties are deliberately absent here and in `property_tests`, and both omissions
//! are load-bearing rather than an oversight:
//!
//! - **No parameter-smoothing property.** Stimulus parameters are frozen for the duration of a
//!   capture, and a smoothed level ramp would corrupt the very calibration the capture exists to
//!   measure. The generators therefore apply parameters immediately.
//! - **No silence-in-silence-out.** These sources ignore their input frames, so the property would
//!   be vacuous — it would pass for a generator that ignored its input by accident.
//!
//! Run with: `cargo test -p asperitas-dsp --test stimulus_tests`

use std::time::Instant;

use asperitas_dsp::{
    ExponentialSweep, ExponentialSweepParams, Frame, Processor, PulseTrain, PulseTrainParams, Sine,
    SineParams, Stimulus,
};

const FS: u32 = 48_000;
const TAU: f64 = core::f64::consts::TAU;

/// The frame budget of a logging body (`asperitas-logging`'s `frame::MAX_BODY`). Hardcoded
/// because `asperitas-dsp` must not depend on logging to learn how big its own description may be.
const MAX_BODY: usize = 200;

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Peak amplitude implied by a peak-referenced dBFS level.
fn amplitude(level_dbfs: f64) -> f64 {
    libm::pow(10.0, level_dbfs / 20.0)
}

fn sine(level_dbfs: f32, frequency_hz: u32) -> Sine {
    let mut s = Sine::default();
    s.set_sample_rate(FS as f32);
    s.set_params(&SineParams {
        level_dbfs,
        frequency_hz,
    });
    s
}

fn sweep(params: ExponentialSweepParams) -> ExponentialSweep {
    let mut e = ExponentialSweep::default();
    e.set_sample_rate(FS as f32);
    e.set_params(&params);
    e
}

fn pulse(params: PulseTrainParams) -> PulseTrain {
    let mut p = PulseTrain::default();
    p.set_sample_rate(FS as f32);
    p.set_params(&params);
    p
}

/// Emit `n` samples from any source and return the left channel. Both channels carry the same
/// value; one is enough to characterise the waveform.
fn emit<T: Processor>(source: &mut T, n: usize) -> Vec<f32> {
    (0..n)
        .map(|_| source.tick(Frame::from([0.0, 0.0]))[0])
        .collect()
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |a, &v| a.max(v.abs()))
}

/// Magnitude of one DFT bin of a period-`samples.len()` record.
///
/// Only single bins are ever evaluated: a full spectrum of a 48,000-sample record is 2.3 billion
/// trig calls, and every claim here needs at most a handful of named bins.
fn bin_magnitude(samples: &[f32], bin: usize) -> f64 {
    let n = samples.len() as f64;
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &sample) in samples.iter().enumerate() {
        let angle = TAU * bin as f64 * i as f64 / n;
        re += f64::from(sample) * libm::cos(angle);
        im -= f64::from(sample) * libm::sin(angle);
    }
    (re * re + im * im).sqrt()
}

// ---------------------------------------------------------------------------
// Sine: exact frequency (AC #2)
// ---------------------------------------------------------------------------

#[test]
fn integer_period_window_contains_integer_cycles() {
    // Frequencies whose period does not divide the window length are the ones that would leak, so
    // 997 Hz (coprime with 48 kHz, one full second per period) is the interesting row.
    for freq in [1u32, 440, 997, 1000, 4800, 8000, 12_345, 16_000, 24_000] {
        let source = sine(-20.0, freq);
        let period = source.period_samples() as u64;

        // The window that matters: a whole number of periods. It then holds a whole number of
        // cycles exactly, in integers, with nothing measured.
        let cycles = u64::from(freq) / u64::from(gcd(freq, FS));
        assert_eq!(
            period * u64::from(freq),
            cycles * u64::from(FS),
            "{freq} Hz: {} samples is not {cycles} whole cycles",
            period
        );

        // And the sequence really repeats with that period, bit for bit, rather than drifting into
        // it: two adjacent windows must be indistinguishable.
        let mut source = sine(-20.0, freq);
        let first = emit(&mut source, period as usize);
        let second = emit(&mut source, period as usize);
        assert!(
            first
                .iter()
                .zip(&second)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "{freq} Hz: output changed after one period of {} samples",
            period
        );

        // A half-period shift must *not* look like the original, or the reported period is twice
        // the real one and the window above holds twice as many cycles as it claims.
        let half = (period / 2) as usize;
        if half > 0 {
            let mut shifted = sine(-20.0, freq);
            let two_periods = emit(&mut shifted, (2 * period) as usize);
            assert!(
                two_periods[..half]
                    .iter()
                    .zip(&two_periods[half..2 * half])
                    .any(|(a, b)| a.to_bits() != b.to_bits()),
                "{freq} Hz: period {} looks twice as long as it is",
                period
            );
        }
    }
}

#[test]
fn frequency_error_is_far_below_the_one_in_ten_million_budget() {
    // A coherent window — one whose length is a whole number of periods — puts all a tone's energy
    // in a single DFT bin. Off-bin leakage from a tone displaced by delta bins shows up almost
    // entirely in the two neighbouring bins, at a ratio of about delta itself: measured against
    // synthetic tones displaced by 1e-9, 1e-7, 1e-5 and 1e-3 bins, the neighbour ratio tracked
    // delta to within 0.1%. So bounding the neighbours bounds the frequency error, and the bound
    // converts to hertz directly because the window below is exactly one second wide.
    const NEIGHBOUR_BUDGET: f64 = 1e-6;

    for freq in [440u32, 997, 1000, 8000, 12_345] {
        let mut source = sine(-20.0, freq);
        // Exactly one second: gcd(f, fs) periods of the tone, whichever frequency is asked for.
        let window = emit(&mut source, FS as usize);
        let signal = bin_magnitude(&window, freq as usize);
        assert!(signal > 0.0, "{freq} Hz produced no spectral energy");

        let lower = bin_magnitude(&window, freq as usize - 1) / signal;
        let upper = bin_magnitude(&window, freq as usize + 1) / signal;
        let displacement_bins = lower.max(upper);
        assert!(
            displacement_bins < NEIGHBOUR_BUDGET,
            "{freq} Hz: neighbour bins at {displacement_bins:.3e} of the peak"
        );

        // Worst case, take the whole relative-error budget on the *lowest* tone tested here.
        let relative_error_bound = displacement_bins / f64::from(freq);
        assert!(
            relative_error_bound < 1e-7,
            "{freq} Hz: frequency error bounded only to {relative_error_bound:.3e} relative"
        );

        // Nothing else may carry energy either: DC, harmonics, image and scattered far bins. A
        // drifting accumulator or a stray offset fails here while surviving the neighbour check.
        let others = [
            0,
            1,
            2,
            freq as usize / 2,
            2 * freq as usize,
            3 * freq as usize,
            FS as usize / 4,
            FS as usize / 3,
            FS as usize - 1,
        ];
        for bin in others {
            if bin == freq as usize || bin == FS as usize - freq as usize {
                continue;
            }
            let ratio = bin_magnitude(&window, bin) / signal;
            assert!(
                ratio < 1e-8,
                "{freq} Hz: bin {bin} carries {ratio:.3e} of the peak"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Amplitude is exact, and stays exact (AC #3)
// ---------------------------------------------------------------------------

#[test]
fn peak_matches_requested_level_across_a_tone_table() {
    // 8000 Hz and 16000 Hz are the regression tripwires. With a zero initial phase they peak 1.25
    // dB low, because no sample lands on the crest; see `crest_alignment_is_why_the_peak_is_exact`.
    for freq in [
        1u32, 20, 997, 1000, 4800, 8000, 11_025, 16_000, 19_200, 24_000,
    ] {
        for level in [-144.0f32, -80.0, -20.0, -6.0, -0.0] {
            let want = amplitude(f64::from(level)) as f32;
            let got = {
                let mut source = sine(level, freq);
                let period = source.period_samples() as usize;
                peak(&emit(&mut source, period))
            };
            assert!(
                (f64::from(got) - f64::from(want)).abs() <= 1e-6 * f64::from(want),
                "{freq} Hz at {level} dBFS: peak {got} instead of {want}"
            );

            // Repeating the run must reproduce it exactly, not to tolerance.
            let repeat = {
                let mut source = sine(level, freq);
                let period = source.period_samples() as usize;
                peak(&emit(&mut source, period))
            };
            assert_eq!(
                got.to_bits(),
                repeat.to_bits(),
                "{freq} Hz not reproducible"
            );
        }
    }
}

#[test]
fn crest_alignment_is_why_the_peak_is_exact() {
    // The generator places its first sample on the crest instead of at phase zero. This test keeps
    // the reason visible: with phi0 = 0 the same exact NCO reports the wrong level, and no amount
    // of accumulator precision fixes it.
    let unaligned_peak = |freq: u32| -> f32 {
        let period = FS / gcd(freq, FS);
        (0..period as usize).fold(0.0f32, |a, n| {
            let residue = (n as u64 * u64::from(freq)) % u64::from(FS);
            let v = (amplitude(-20.0) * libm::sin(TAU * residue as f64 / f64::from(FS))) as f32;
            a.max(v.abs())
        })
    };

    for freq in [8000u32, 16_000, 19_200] {
        let unaligned = unaligned_peak(freq);
        let aligned = peak(&emit(&mut sine(-20.0, freq), (FS / gcd(freq, FS)) as usize));
        let shortfall_db = 20.0 * libm::log10(f64::from(aligned) / f64::from(unaligned).max(1e-12));
        assert!(
            shortfall_db > 0.2,
            "{freq} Hz: an unaligned start now costs only {shortfall_db:.3} dB, so this test's \
             premise has expired"
        );
    }

    // Nyquist is the degenerate case: an unaligned start there emits identically zero.
    let nyquist_unaligned = unaligned_peak(24_000);
    assert!(
        nyquist_unaligned < 1e-9,
        "expected 24 kHz with phi0 = 0 to be silent, found peak {nyquist_unaligned}"
    );
}

#[test]
fn level_is_unchanged_after_ten_minutes() {
    // AC #3's horizon: ten minutes at 48 kHz. Every tick goes through the real audio path, because
    // the claim is precisely that the shipped accumulator does not sag — reproducing its arithmetic
    // in the test would assert a model rather than the code. Cost is ~2 s in a debug build.
    const TOTAL: usize = 28_800_000;
    const EDGE: usize = 48_000;

    let started = Instant::now();
    let mut source = sine(-20.0, 997);
    let head = emit(&mut source, EDGE);
    let head_peak = peak(&head);
    emit(&mut source, TOTAL - 2 * EDGE);
    let tail = emit(&mut source, EDGE);
    let tail_peak = peak(&tail);

    assert_eq!(
        head_peak.to_bits(),
        tail_peak.to_bits(),
        "peak changed between the first and last minute of a ten-minute run"
    );
    assert!(
        (f64::from(head_peak) - amplitude(-20.0)).abs() <= 1e-6 * amplitude(-20.0),
        "ten minutes in, the peak is {head_peak} rather than {}",
        amplitude(-20.0)
    );
    // Every claim above is about magnitude; this one is about identity. The skip length is a whole
    // multiple of 48 kHz, so the tail window starts on the same phase as the head one — the useful
    // comparison is therefore against a generator that never ran, not against a different phase.
    // A float accumulator that drifted by even one LSB per sample would separate all three.
    assert!(
        head.iter()
            .zip(&tail)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "ten minutes apart, the same phase produced different samples"
    );
    let reference = emit(&mut sine(-20.0, 997), EDGE);
    assert!(
        tail.iter()
            .zip(&reference)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "ten minutes in, output diverged from a generator that never ran"
    );
    println!("ten-minute run took {:?}", started.elapsed());
}

#[test]
fn lowering_the_sample_rate_reclamps_the_frequency() {
    // Regression: `set_sample_rate` used to recompute the phase from a stale frequency. Asking for
    // 24 kHz at 48 kHz and then moving the codec to 8 kHz left a 24 kHz increment driving a
    // 48 kHz-wide wrap, which emitted a constant 0.1 for 100,000 samples and overflowed the
    // accumulator somewhere past 268,435 of them — a panic in the audio interrupt.
    let mut source = sine(-20.0, 24_000);
    source.set_sample_rate(8_000.0);

    let samples = emit(&mut source, 1_000_000);
    assert!(samples.iter().all(|v| v.is_finite()));
    assert_eq!(
        peak(&samples).to_bits(),
        (amplitude(-20.0) as f32).to_bits(),
        "frequency was not clamped to the new Nyquist limit"
    );
    assert_eq!(source.period_samples(), 2, "clamped tone should alternate");
}

// ---------------------------------------------------------------------------
// Exponential sweep (AC #4)
// ---------------------------------------------------------------------------

#[test]
fn sweep_length_is_the_exact_integer_requested() {
    for requested in [1u32, 2, 3, 960, 3_840, 384_000] {
        let params = ExponentialSweepParams {
            level_dbfs: -20.0,
            f0_hz: 20.0,
            f1_hz: 20_000.0,
            total_samples: requested,
        };
        let mut source = sweep(params);
        assert_eq!(source.total_samples(), requested);

        // And the generator stops emitting exactly there, so device and host agree on where the
        // record ends without either counting samples.
        let played = emit(&mut source, requested as usize + 8);
        assert!(played[..requested as usize].iter().all(|v| v.is_finite()));
        assert!(
            played[requested as usize..].iter().all(|&v| v == 0.0),
            "emitted past the end of a {requested}-sample record"
        );
    }

    // Zero is the one request that cannot be honoured as stated; it becomes a one-sample record
    // rather than a division by a zero length.
    let clamped = sweep(ExponentialSweepParams {
        level_dbfs: -20.0,
        f0_hz: 20.0,
        f1_hz: 20_000.0,
        total_samples: 0,
    });
    assert_eq!(clamped.total_samples(), 1);
}

#[test]
fn sweep_spans_its_endpoints_exponentially() {
    let mut source = sweep(ExponentialSweepParams::default());
    let params = ExponentialSweepParams::default();
    let n = source.total_samples() as usize;
    let samples = emit(&mut source, n);

    // Counting rising zero crossings locates cycles without assuming anything about phase. For an
    // exponential chirp the count accumulated by the halfway point is 1/(1+sqrt(r)) of the total,
    // because equal *times* cover equal *ratios*; a linear chirp of the same endpoints gives a
    // third of the cycles in the first half. Measured: 0.03166 against 0.03162 predicted.
    let mut crossings = 0usize;
    let mut first_half = 0usize;
    for i in 1..n {
        if samples[i - 1] <= 0.0 && samples[i] > 0.0 {
            crossings += 1;
            if i < n / 2 {
                first_half += 1;
            }
        }
    }
    let ratio = f64::from(params.f1_hz) / f64::from(params.f0_hz);
    let measured = first_half as f64 / (crossings - first_half) as f64;
    let exponential = 1.0 / ratio.sqrt();
    let linear = 1.0 / 3.0;
    assert!(
        (measured - exponential).abs() < 0.02 * exponential,
        "first-half share {measured:.6} is not the exponential {exponential:.6}"
    );
    assert!(
        (measured - linear).abs() > 0.1,
        "sweep looks linear ({measured:.6}), not exponential"
    );

    // Endpoints. Instantaneous frequency at sample n is f0·r^(n/N), so the endpoint *time* T is
    // never emitted: the last sample sits just under f1. Asserting the documented convention
    // rather than the value keeps the wire format free of a derived field.
    let last = f64::from(source.instantaneous_frequency_hz(n as u32 - 1));
    let expected_last = f64::from(params.f0_hz) * libm::pow(ratio, (n - 1) as f64 / n as f64);
    assert!(
        (last - expected_last).abs() < 1e-3 * expected_last,
        "last sample at {last} Hz instead of {expected_last} Hz"
    );
    assert!(
        last < f64::from(params.f1_hz) && last > 0.999 * f64::from(params.f1_hz),
        "endpoint convention drifted: {last} Hz"
    );
    assert!(
        (f64::from(source.instantaneous_frequency_hz(0)) - f64::from(params.f0_hz)).abs() < 1e-6,
        "sweep does not start at f0"
    );
}

#[test]
fn sweep_starts_and_ends_on_exact_silence() {
    // The raised-cosine taper is what keeps a finished sweep from exciting the loop with a step.
    // An exact zero at both boundaries is stronger than "small", and the closed-form window gives
    // it for free, so there is no reason to accept a tolerance.
    let mut source = sweep(ExponentialSweepParams::default());
    let n = source.total_samples() as usize;
    let samples = emit(&mut source, n);
    assert_eq!(samples[0], 0.0);
    assert_eq!(samples[n - 1], 0.0);
    assert!(f64::from(samples[1]).abs() < 1e-8, "second sample leaks");
    assert!(
        f64::from(samples[n - 2]).abs() < 1e-5,
        "second-to-last sample leaks"
    );

    // What the taper replaces: the same closed form evaluated without the window ends mid-swing,
    // at −0.963 — effectively a step, which is what Farina's AES 122 note warns the fade-out is for.
    let params = ExponentialSweepParams::default();
    let ratio = f64::from(params.f1_hz) / f64::from(params.f0_hz);
    let duration = n as f64 / f64::from(FS);
    let final_phase = TAU
        * f64::from(params.f0_hz)
        * duration
        * (libm::pow(ratio, (n - 1) as f64 / n as f64) - 1.0)
        / libm::log(ratio);
    let untapered_final = libm::sin(final_phase);
    assert!(
        untapered_final < -0.9,
        "the untapered record no longer ends near full scale: {untapered_final}"
    );
}

#[test]
fn sweep_carries_equal_energy_per_octave_because_it_is_not_pre_emphasised() {
    // Farina's refinement is that the ±3 dB/octave weighting belongs on the inverse filter, not on
    // the played stimulus: pre-emphasis time-smears the recovered response. If anyone adds it back,
    // octave maxima stop being equal and this fails.
    let params = ExponentialSweepParams::default();
    let mut source = sweep(params.clone());
    let n = source.total_samples() as usize;
    let samples = emit(&mut source, n);

    let ratio = f64::from(params.f1_hz) / f64::from(params.f0_hz);
    let overall = peak(&samples);
    // Stay clear of the tapered margins, where unequal maxima would be legitimate.
    let lo_guard = 480;
    let hi_guard = n - 480;

    let mut loudest_octave = 0.0f64;
    let mut quietest_octave = f64::INFINITY;
    for octave in 0..10 {
        let edge =
            |k: i32| (n as f64 * (libm::log(2.0f64.powi(k)) / libm::log(ratio))).round() as usize;
        let start = edge(octave).max(lo_guard);
        let end = edge(octave + 1).min(hi_guard);
        if end <= start {
            continue;
        }
        let band = peak(&samples[start..end]) as f64;
        loudest_octave = loudest_octave.max(band);
        quietest_octave = quietest_octave.min(band);
    }

    // Measured spread across the ten octaves of the default sweep is bit-for-bit zero; 1e-6 relative
    // is rounding headroom, while the ±3 dB/octave weighting this rules out moves a decade by decades.
    assert!(
        (loudest_octave - quietest_octave) <= 1e-6 * loudest_octave,
        "octave maxima differ: {loudest_octave} vs {quietest_octave}"
    );
    assert!(
        (loudest_octave - f64::from(overall)).abs() <= 1e-6 * loudest_octave,
        "no octave band carries the record's peak ({loudest_octave} vs {})",
        f64::from(overall)
    );
}

#[test]
fn sweep_peak_matches_requested_level_for_arbitrary_endpoints() {
    // The peak normalisation is a scan, not a closed form, so it must hold for endpoints nobody
    // measured rather than only for the defaults that happened to work.
    let cases: [(f32, f32, f32, u32); 5] = [
        (-20.0, 20.0, 20_000.0, 384_000),
        (-6.0, 100.0, 4_000.0, 1_000),
        (-3.0, 50.0, 12_000.0, 50_000),
        (-80.0, 1_000.0, 1_001.0, 20_000),
        (-0.0, 20.0, 24_000.0, 100_000),
    ];
    for (level, f0, f1, total) in cases {
        let mut source = sweep(ExponentialSweepParams {
            level_dbfs: level,
            f0_hz: f0,
            f1_hz: f1,
            total_samples: total,
        });
        let samples = emit(&mut source, total as usize);
        let want = amplitude(f64::from(level)) as f32;
        let got = peak(&samples);
        assert!(
            (f64::from(got) - f64::from(want)).abs() <= 1e-6 * f64::from(want),
            "{f0}-{f1} Hz over {total} samples at {level} dBFS: peak {got}, want {want}"
        );
    }
}

// ---------------------------------------------------------------------------
// Band-limited pulse train (AC #4)
// ---------------------------------------------------------------------------

#[test]
fn pulse_is_band_limited_where_a_click_is_not() {
    let params = PulseTrainParams::default();
    let mut source = pulse(params.clone());
    let period = params.period_samples as usize;
    let k = source.harmonic_count() as usize;
    let samples = emit(&mut source, period);

    let mut loudest = 0.0f64;
    for bin in 0..=k {
        loudest = loudest.max(bin_magnitude(&samples, bin));
    }
    let mut worst_forbidden = 0.0f64;
    for bin in k + 1..period - k {
        worst_forbidden = worst_forbidden.max(bin_magnitude(&samples, bin) / loudest);
    }
    // Samples are stored as f32, so rounding alone lifts an empty bin to ~1e-8 of the peak. Three
    // decades of slack still rejects anything with real out-of-band content, as the click below
    // shows.
    assert!(
        worst_forbidden < 1e-5,
        "bins above the {k}th harmonic carry {worst_forbidden:.3e} of the peak"
    );

    // The reference a naive implementation would produce: one sample, flat to Nyquist by
    // construction, which is exactly where an analog loop is least trustworthy.
    let period = params.period_samples as usize;
    let mut click = vec![0.0f32; period];
    click[0] = 0.1;
    let click_loudest = bin_magnitude(&click, 0);
    let quietest_forbidden = (k + 1..period - k)
        .map(|bin| bin_magnitude(&click, bin) / click_loudest)
        .fold(f64::INFINITY, f64::min);
    assert!(
        quietest_forbidden > 0.5,
        "the click reference stopped looking flat: {quietest_forbidden}"
    );
}

#[test]
fn pulse_peak_is_exact_and_its_energy_is_analytic() {
    let params = PulseTrainParams::default();
    let mut source = pulse(params.clone());
    let period = params.period_samples as usize;
    let k = source.harmonic_count() as usize;
    let samples = emit(&mut source, period);

    // Peak-normalised by construction: the kernel's limit at its crest is 1.0, so the emitted peak
    // is the requested level with no normalisation pass behind it.
    assert_eq!(
        peak(&samples).to_bits(),
        (amplitude(f64::from(params.level_dbfs)) as f32).to_bits()
    );

    // Energy in the crest sample over total energy is (2K+1)/N analytically, via Parseval.
    let total: f64 = samples.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let crest_fraction = f64::from(samples[0]) * f64::from(samples[0]) / total;
    let analytic = (2 * k + 1) as f64 / period as f64;
    // Samples are f32, so the summands carry ~1e-9 relative error each; measured disagreement with
    // the analytic value is 2.1e-8 absolute.
    assert!(
        (crest_fraction - analytic).abs() < 1e-6,
        "crest holds {crest_fraction:.6} of the energy, analytics say {analytic:.6}"
    );

    // Spread, asserted against the click AC #4 rejects: three samples reach half the crest here,
    // one reaches it for a delta.
    let spread = samples
        .iter()
        .filter(|&&v| v.abs() * 2.0 >= peak(&samples))
        .count();
    assert_eq!(spread, 3, "pulse is not spread over more than one sample");
    let mut click = vec![0.0f32; period];
    click[0] = 0.1;
    let click_spread = click.iter().filter(|&&v| v.abs() * 2.0 >= 0.1).count();
    assert_eq!(click_spread, 1);
}

#[test]
fn pulse_repeats_bit_for_bit_every_period() {
    let params = PulseTrainParams {
        level_dbfs: -12.0,
        period_samples: 1_000,
        max_frequency_hz: 3_000.0,
    };
    let mut source = pulse(params.clone());
    let period = params.period_samples as usize;
    let first = emit(&mut source, period);
    let rest = emit(&mut source, 4 * period);
    assert!(
        first
            .iter()
            .cycle()
            .zip(&rest)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "pulse train is not exactly periodic"
    );
}

#[test]
fn harmonic_count_clamp_blocks_the_delta_degeneracy() {
    // As K approaches (N-1)/2 the sampled Dirichlet kernel degenerates toward a one-sample delta,
    // which is the stimulus this ticket rejects. The clamp caps K there, and the shape difference
    // is what makes the cap worth testing.
    let mut source = pulse(PulseTrainParams {
        level_dbfs: -20.0,
        period_samples: 480,
        max_frequency_hz: 24_000.0,
    });
    assert_eq!(source.harmonic_count(), 239, "clamp is not at (N-1)/2");
    let samples = emit(&mut source, 480);
    let total: f64 = samples.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let crest_fraction = f64::from(samples[0]) * f64::from(samples[0]) / total;
    assert!(
        crest_fraction > 0.99,
        "the clamped kernel no longer degenerates: {crest_fraction:.6}"
    );

    // Against the chosen defaults, where the crest holds a third of the energy instead of all of it.
    let mut tuned = pulse(PulseTrainParams::default());
    let tuned_samples = emit(&mut tuned, 480);
    let tuned_total: f64 = tuned_samples
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum();
    let tuned_fraction = f64::from(tuned_samples[0]) * f64::from(tuned_samples[0]) / tuned_total;
    assert!(
        tuned_fraction < 0.4,
        "default pulse became click-like: {tuned_fraction:.6}"
    );

    // K is physical units in, integer arithmetic out.
    let cases: [(u32, f32, u32); 4] = [
        (480, 8_000.0, 80),
        (960, 8_000.0, 160),
        (480, 100.0, 1),
        (1_000, 3_000.0, 62),
    ];
    for (period_samples, max_hz, want) in cases {
        let source = pulse(PulseTrainParams {
            level_dbfs: -20.0,
            period_samples,
            max_frequency_hz: max_hz,
        });
        assert_eq!(
            source.harmonic_count(),
            want,
            "period {period_samples} at {max_hz} Hz"
        );
    }
}

#[test]
fn pulse_carries_one_harmonic_of_dc_and_says_so() {
    // The Dirichlet kernel here is `D_K/(2K+1)`, and `D_K` includes its constant term, so the pulse
    // is not zero-mean: it carries a DC offset worth exactly one harmonic of its spectrum. That is
    // harmless for deconvolution — the host reproduces it from the same source — but a caller
    // comparing against an AC-coupled meter needs to know it is there. Pinned so the number in the
    // module docs cannot rot.
    let params = PulseTrainParams::default();
    let mut source = pulse(params.clone());
    let period = params.period_samples as usize;
    let samples = emit(&mut source, period);
    let mean = samples.iter().map(|&v| f64::from(v)).sum::<f64>() / period as f64;
    let want = amplitude(-20.0) / (2.0 * f64::from(source.harmonic_count()) + 1.0);
    assert!(
        (mean - want).abs() < 1e-9,
        "DC term {mean:.3e}, expected one harmonic at {want:.3e}"
    );

    // Same fact in the spectrum: the DC bin is exactly as loud as the highest retained harmonic,
    // which puts the offset 44 dB under the crest for the default K.
    let dc = bin_magnitude(&samples, 0);
    let loudest_harmonic = bin_magnitude(&samples, source.harmonic_count() as usize);
    assert!(
        (dc - loudest_harmonic).abs() < 1e-6 * dc,
        "DC bin {dc} is not one harmonic ({loudest_harmonic})"
    );
    let below_crest_db = 20.0 * libm::log10(f64::from(peak(&samples)) / mean);
    assert!(
        below_crest_db > 40.0 && below_crest_db < 50.0,
        "DC offset now sits {below_crest_db:.1} dB under the crest"
    );
}

// ---------------------------------------------------------------------------
// Degenerate parameters
// ---------------------------------------------------------------------------

#[test]
fn degenerate_parameters_become_ordinary_output() {
    // The crate's contract is clamp, saturate, and never return a Result. Generators get less
    // exercise than gain and filter from proptest, because their interesting inputs are parameter
    // values rather than audio frames.
    for level in [
        -f32::INFINITY,
        -1e9,
        -145.0,
        0.0,
        6.0,
        f32::INFINITY,
        f32::NAN,
    ] {
        for freq in [0u32, 1, 24_000, 24_001, u32::MAX] {
            let mut source = sine(level, freq);
            let samples = emit(&mut source, 500);
            assert!(samples.iter().all(|v| v.is_finite()), "sine {level}/{freq}");
            assert!(
                peak(&samples) <= 1.0,
                "sine {level}/{freq} exceeded full scale"
            );
        }
    }

    for rate in [0.0f32, -1.0, f32::NAN, f32::INFINITY, 44_100.0, 8_000.0] {
        let mut source = Sine::default();
        source.set_sample_rate(rate);
        source.set_params(&SineParams::default());
        let samples = emit(&mut source, 500);
        assert!(samples.iter().all(|v| v.is_finite()), "sine at {rate} Hz");
    }

    let bad_levels = [-f32::INFINITY, 6.0, f32::NAN];
    let bad_bands: [(f32, f32); 5] = [
        (0.0, 20_000.0),
        (20_000.0, 20.0),
        (f32::NAN, 20_000.0),
        (1e-9, 1e9),
        (24_000.0, 24_000.0),
    ];
    for level in bad_levels {
        for (f0, f1) in bad_bands {
            for total in [1u32, 2, 3, 5_000] {
                let mut source = sweep(ExponentialSweepParams {
                    level_dbfs: level,
                    f0_hz: f0,
                    f1_hz: f1,
                    total_samples: total,
                });
                let samples = emit(&mut source, total as usize + 4);
                assert!(
                    samples.iter().all(|v| v.is_finite()),
                    "sweep {level} dBFS, {f0}-{f1} Hz, {total} samples"
                );
                assert!(peak(&samples) <= 1.0);
            }
        }
    }

    for level in bad_levels {
        for (period, max_hz) in [
            (0u32, 8_000.0f32),
            (1, 8_000.0),
            (2, 24_000.0),
            (480, -1.0),
            (480, f32::NAN),
            (480, 1e9),
            (u32::MAX, 8_000.0),
        ] {
            let mut source = pulse(PulseTrainParams {
                level_dbfs: level,
                period_samples: period,
                max_frequency_hz: max_hz,
            });
            let samples = emit(&mut source, 500);
            assert!(
                samples.iter().all(|v| v.is_finite()),
                "pulse {level} dBFS, period {period}, {max_hz} Hz"
            );
            assert!(peak(&samples) <= 1.0);
        }
    }
}

// ---------------------------------------------------------------------------
// describe(): the machine-readable record of what was played (AC #5)
// ---------------------------------------------------------------------------

#[test]
fn describe_strings_are_pinned() {
    // Renaming a field, reordering them, or changing a float's rendering desynchronises the device
    // record from whatever the host reconstructs, and both sides keep passing their own tests. The
    // literals below are that failure's tripwire.
    let mut buf = [0u8; 256];

    let source = sine(-20.0, 1_000);
    let n = source.describe(&mut buf);
    assert_eq!(
        core::str::from_utf8(&buf[..n]).unwrap(),
        "name=sine sample_rate_hz=48000 level_dbfs=-20.0 frequency_hz=1000 period_samples=48"
    );

    let source = sweep(ExponentialSweepParams::default());
    let n = source.describe(&mut buf);
    assert_eq!(
        core::str::from_utf8(&buf[..n]).unwrap(),
        "name=ess sample_rate_hz=48000 level_dbfs=-20.0 f0_hz=20.0 f1_hz=20000.0 total_samples=384000"
    );

    let source = pulse(PulseTrainParams::default());
    let n = source.describe(&mut buf);
    assert_eq!(
        core::str::from_utf8(&buf[..n]).unwrap(),
        "name=pulse_train sample_rate_hz=48000 level_dbfs=-20.0 period_samples=480 max_frequency_hz=8000.0"
    );
}

#[test]
fn describe_respects_the_frame_budget_and_truncates_rather_than_panicking() {
    // Worst case per field: the widest level, the widest integer each source can render after
    // clamping, and the longest names.
    let mut buf = [0u8; 256];
    let mut worst = 0usize;

    let source = sine(-144.0, 1);
    worst = worst.max(source.describe(&mut buf));
    // Six digits rather than the eight an `total_samples` can reach, because building a sweep runs
    // the peak scan over the whole record and a 28.8M-sample sweep costs ~14 s in a debug build.
    // The two spare digits are paid for explicitly below instead.
    let source = sweep(ExponentialSweepParams {
        level_dbfs: -144.0,
        f0_hz: 1.0,
        f1_hz: 24_000.0,
        total_samples: 999_999,
    });
    worst = worst.max(source.describe(&mut buf));
    let source = pulse(PulseTrainParams {
        level_dbfs: -144.0,
        period_samples: 48_000_000,
        max_frequency_hz: 24_000.0,
    });
    worst = worst.max(source.describe(&mut buf));

    assert!(
        worst + 2 < MAX_BODY,
        "worst-case description is {worst} bytes plus two spare digits, budget is {MAX_BODY}"
    );

    // Grammar rules the console's framing depends on: single spaces, no trailing space, and no
    // CR or LF, because the frame itself supplies the delimiter.
    let n = sine(-20.0, 997).describe(&mut buf);
    let text = core::str::from_utf8(&buf[..n]).unwrap();
    assert!(!text.contains('\r') && !text.contains('\n'));
    assert!(!text.contains("  "), "double space in {text:?}");
    assert!(!text.starts_with(' ') && !text.ends_with(' '), "{text:?}");
    assert_eq!(text.split(' ').count(), 5, "field count changed: {text:?}");

    // A buffer too small truncates silently instead of panicking in the audio path.
    let mut tiny = [0u8; 8];
    let n = sine(-20.0, 1_000).describe(&mut tiny);
    assert_eq!(n, 8);
    assert_eq!(core::str::from_utf8(&tiny).unwrap(), "name=sin");

    // Zero-length buffer: writes nothing, claims nothing.
    let mut empty: [u8; 0] = [];
    assert_eq!(sine(-20.0, 1_000).describe(&mut empty), 0);
}

// ---------------------------------------------------------------------------
// Reset
// ---------------------------------------------------------------------------

#[test]
fn reset_replays_bit_identically_for_every_source() {
    let mut source = sine(-6.0, 997);
    emit(&mut source, 1_234);
    source.reset();
    let replayed = emit(&mut source, 2_000);
    let fresh = emit(&mut sine(-6.0, 997), 2_000);
    assert!(replayed
        .iter()
        .zip(&fresh)
        .all(|(a, b)| a.to_bits() == b.to_bits()));

    let params = ExponentialSweepParams {
        level_dbfs: -20.0,
        f0_hz: 100.0,
        f1_hz: 8_000.0,
        total_samples: 10_000,
    };
    let mut source = sweep(params.clone());
    emit(&mut source, 1_234);
    source.reset();
    let replayed = emit(&mut source, 2_000);
    let fresh = emit(&mut sweep(params), 2_000);
    assert!(replayed
        .iter()
        .zip(&fresh)
        .all(|(a, b)| a.to_bits() == b.to_bits()));

    let params = PulseTrainParams {
        level_dbfs: -12.0,
        period_samples: 1_000,
        max_frequency_hz: 3_000.0,
    };
    let mut source = pulse(params.clone());
    emit(&mut source, 777);
    source.reset();
    let replayed = emit(&mut source, 2_000);
    let fresh = emit(&mut pulse(params), 2_000);
    assert!(replayed
        .iter()
        .zip(&fresh)
        .all(|(a, b)| a.to_bits() == b.to_bits()));
}
