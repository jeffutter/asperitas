//! Behaviour of the pitch tracker on synthesised notes, silence, noise and chords.
//!
//! The tolerances and settle times asserted here are the ones documented in
//! `docs/reference/pitch-tracker.md`.

#![cfg(feature = "pitch")]

use asperitas_dsp::{PitchParams, PitchTracker};
use std::f64::consts::PI;

const RATE: f32 = 48_000.0;
const BLOCK: usize = 32;
const NOTES: [f32; 7] = [41.0, 82.0, 110.0, 220.0, 440.0, 880.0, 1760.0];

/// Cents tolerance on a settled estimate.
const TOLERANCE_CENTS: f64 = 10.0;
/// Settle time in ms: after this long every block's estimate is within tolerance.
const SETTLE_MS: f64 = 100.0;
/// Confidence ceiling on silence, white noise and a dense chord.
const REJECT_CONFIDENCE: f32 = 0.8;

#[derive(Clone, Copy)]
enum Timbre {
    Sine,
    Saw,
    Pluck,
}

/// A note starting at sample 0 as an additive sum of partials below 20 kHz. `Pluck` gives each
/// partial `k` an amplitude `1/k` and a decay of `1.5 + 0.8 k` per second.
fn note(timbre: Timbre, hz: f64, len: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; len];
    let partials = ((20_000.0 / hz) as usize).max(1);
    for (n, o) in out.iter_mut().enumerate() {
        let t = n as f64 / RATE as f64;
        let mut v = 0.0;
        match timbre {
            Timbre::Sine => v = (2.0 * PI * hz * t).sin(),
            Timbre::Saw => {
                for k in 1..=partials {
                    v += (2.0 * PI * hz * k as f64 * t).sin() / k as f64;
                }
                v *= 0.5;
            }
            Timbre::Pluck => {
                for k in 1..=partials.min(20) {
                    let kf = k as f64;
                    v += (2.0 * PI * hz * kf * t).sin() / kf * (-(1.5 + 0.8 * kf) * t).exp();
                }
                v *= 0.5;
            }
        }
        *o = (0.5 * v) as f32;
    }
    out
}

fn cents(a: f32, b: f32) -> f64 {
    1200.0 * ((a as f64) / (b as f64)).log2()
}

/// Per-block `(time_ms, estimate, confidence)` after each block of `input`.
fn trace(input: &[f32]) -> Vec<(f64, f32, f32)> {
    let mut t = PitchTracker::new(RATE, &PitchParams::default());
    input
        .chunks_exact(BLOCK)
        .enumerate()
        .map(|(i, b)| {
            t.process_block(b);
            (
                (i + 1) as f64 * BLOCK as f64 * 1000.0 / RATE as f64,
                t.estimate_hz(),
                t.confidence(),
            )
        })
        .collect()
}

/// Time in ms after which every block's estimate is within `TOLERANCE_CENTS` of `hz`, up to the
/// end of the trace; `None` when the last block is outside it.
fn settle_ms(tr: &[(f64, f32, f32)], hz: f32) -> Option<f64> {
    let ok = |e: f32| e > 0.0 && cents(e, hz).abs() <= TOLERANCE_CENTS;
    if !ok(tr.last()?.1) {
        return None;
    }
    let last_bad = tr.iter().rposition(|p| !ok(p.1));
    Some(last_bad.map_or(0.0, |i| tr[i + 1].0))
}

#[test]
fn notes_settle_within_tolerance() {
    for (name, timbre) in [
        ("sine", Timbre::Sine),
        ("saw", Timbre::Saw),
        ("pluck", Timbre::Pluck),
    ] {
        for hz in NOTES {
            // A pluck decays, so hold the comparison to the first second only.
            let len = 48_000;
            let tr = trace(&note(timbre, hz as f64, len));
            let s = settle_ms(&tr, hz);
            if std::env::var("PITCH_REPORT").is_ok() {
                let worst = tr
                    .iter()
                    .filter(|p| p.0 >= SETTLE_MS)
                    .map(|p| cents(p.1, hz).abs())
                    .fold(0.0, f64::max);
                println!("{name} {hz} Hz: settle {s:?} ms, worst cents after settle {worst:.2}");
            }
            let s = s.unwrap_or_else(|| panic!("{name} {hz} Hz never settled"));
            assert!(s <= SETTLE_MS, "{name} {hz} Hz settled at {s} ms");
        }
    }
}

fn lcg(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1 << 23) as f32 - 1.0
}

/// Highest confidence over the trace once the windows are full (after 200 ms).
fn peak_confidence(input: &[f32]) -> f32 {
    trace(input)
        .iter()
        .filter(|p| p.0 >= 200.0)
        .fold(0.0, |m, p| m.max(p.2))
}

#[test]
fn silence_has_zero_confidence_and_no_estimate() {
    let tr = trace(&vec![0.0; 48_000]);
    assert!(tr.iter().all(|p| p.1 == 0.0 && p.2 == 0.0));
}

#[test]
fn white_noise_stays_below_the_rejection_ceiling() {
    for (seed0, amp) in [(1u32, 0.5f32), (7, 0.05), (99, 0.9)] {
        let mut seed = seed0;
        let x: Vec<f32> = (0..96_000).map(|_| amp * lcg(&mut seed)).collect();
        let c = peak_confidence(&x);
        if std::env::var("PITCH_REPORT").is_ok() {
            println!("noise seed {seed0} amp {amp}: peak confidence {c:.3}");
        }
        assert!(c < REJECT_CONFIDENCE, "noise {seed0}/{amp}: {c}");
    }
}

/// A chord of equal-tempered notes with five harmonics each.
fn chord(freqs: &[f64], len: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; len];
    for (n, o) in out.iter_mut().enumerate() {
        let t = n as f64 / RATE as f64;
        let mut v = 0.0;
        for &f in freqs {
            for k in 1..=5 {
                v += (2.0 * PI * f * k as f64 * t).sin() / k as f64;
            }
        }
        *o = (0.15 * v / freqs.len() as f64 * 3.0) as f32;
    }
    out
}

#[test]
fn dense_chords_stay_below_the_rejection_ceiling() {
    // Stacked fourths (open guitar strings), a dominant-ninth voicing and a diminished seventh.
    let chords: [&[f64]; 3] = [
        &[82.41, 110.0, 146.83, 196.0, 246.94, 329.63],
        &[130.81, 164.81, 196.0, 233.08, 293.66],
        &[130.81, 155.56, 185.0, 220.0],
    ];
    for c in chords {
        let conf = peak_confidence(&chord(c, 96_000));
        if std::env::var("PITCH_REPORT").is_ok() {
            println!("chord {c:?}: peak confidence {conf:.3}");
        }
        assert!(conf < REJECT_CONFIDENCE, "chord {c:?}: {conf}");
    }
}

#[test]
fn reset_is_idempotent_and_deterministic() {
    let x = note(Timbre::Pluck, 110.0, 24_000);
    let run = |t: &mut PitchTracker| -> Vec<(u32, u32)> {
        x.chunks_exact(BLOCK)
            .map(|b| {
                t.process_block(b);
                (t.estimate_hz().to_bits(), t.confidence().to_bits())
            })
            .collect()
    };
    let mut fresh = PitchTracker::new(RATE, &PitchParams::default());
    let want = run(&mut fresh);

    let mut used = PitchTracker::new(RATE, &PitchParams::default());
    used.process_block(&note(Timbre::Saw, 330.0, 9_999));
    used.reset();
    used.reset();
    assert_eq!(run(&mut used), want);
    // Reset after the run reproduces it again.
    used.reset();
    assert_eq!(run(&mut used), want);
}

#[test]
fn process_block_equals_repeated_tick() {
    let x = note(Timbre::Pluck, 220.0, 20_000);
    let mut a = PitchTracker::new(RATE, &PitchParams::default());
    let mut b = PitchTracker::new(RATE, &PitchParams::default());
    for chunk in x.chunks(37) {
        a.process_block(chunk);
        for &s in chunk {
            b.tick(s);
        }
        assert_eq!(a.estimate_hz().to_bits(), b.estimate_hz().to_bits());
        assert_eq!(a.confidence().to_bits(), b.confidence().to_bits());
    }
}

#[test]
fn onset_latency_per_note() {
    // A note that starts at 100 ms of silence; latency is from its start to the first block whose
    // estimate is within tolerance, and it must be at most the settle time.
    let lead = 4_800;
    let mut report = Vec::new();
    for hz in NOTES {
        let mut x = vec![0.0f32; lead];
        x.extend(note(Timbre::Pluck, hz as f64, 24_000));
        let tr = trace(&x);
        let first = tr
            .iter()
            .find(|p| p.0 > 100.0 && p.1 > 0.0 && cents(p.1, hz).abs() <= TOLERANCE_CENTS)
            .unwrap_or_else(|| panic!("{hz} Hz never found"));
        let latency = first.0 - 100.0;
        report.push((hz, latency));
        assert!(latency <= SETTLE_MS, "{hz} Hz latency {latency}");
    }
    if std::env::var("PITCH_REPORT").is_ok() {
        for (hz, l) in report {
            println!("onset {hz} Hz: {l:.1} ms");
        }
    }
}

/// A chromatic cluster inside one 64 ms analysis window is a single beating tone, and a major
/// triad is a harmonic series on its root. Neither is rejected: this records what they read as.
#[test]
fn known_limits_chords_that_read_as_one_pitch() {
    let report = std::env::var("PITCH_REPORT").is_ok();
    let cluster = trace(&chord(&[261.63, 277.18, 293.66, 311.13], 96_000));
    let triad = trace(&chord(&[130.81, 164.81, 196.0], 96_000));
    for (name, tr) in [("cluster", cluster), ("major triad", triad)] {
        let last = tr.last().unwrap();
        if report {
            println!("{name}: estimate {:.1} Hz confidence {:.3}", last.1, last.2);
        }
        assert!(
            last.1 == 0.0 || (60.0..330.0).contains(&last.1),
            "{name}: {}",
            last.1
        );
    }
}
