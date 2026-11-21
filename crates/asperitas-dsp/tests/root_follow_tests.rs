//! Behaviour of the root-follow stage fed by the pitch tracker on synthesised input, and the
//! bit-identity of the chain with tracking off. The numbers asserted are the ones documented in
//! `docs/reference/resonator.md`.

#![cfg(feature = "pitch")]

use asperitas_dsp::{
    PitchParams, PitchTracker, Processor, Resonator, ResonatorParams, RootFollow, RootFollowParams,
    RESONATOR_PARAMS,
};
use std::f64::consts::PI;

const RATE: f32 = 48_000.0;
const BLOCK: usize = 32;

/// Time in ms after a note change by which the root is within `LAND_CENTS` of the new note.
const FOLLOW_MS: f64 = 175.0;
const LAND_CENTS: f64 = 20.0;

fn cents(a: f32, b: f32) -> f64 {
    1200.0 * (f64::from(a) / f64::from(b)).log2()
}

fn sine(hz: f64, secs: f64) -> Vec<f32> {
    (0..(secs * f64::from(RATE)) as usize)
        .map(|n| (0.5 * (2.0 * PI * hz * n as f64 / f64::from(RATE)).sin()) as f32)
        .collect()
}

fn noise(secs: f64) -> Vec<f32> {
    let mut state = 0x1234_5678u32;
    (0..(secs * f64::from(RATE)) as usize)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(state >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0) as f32 * 0.35
        })
        .collect()
}

fn chord(secs: f64) -> Vec<f32> {
    let hzs = [130.81, 164.81, 196.0, 233.08, 293.66];
    (0..(secs * f64::from(RATE)) as usize)
        .map(|n| {
            let t = n as f64 / f64::from(RATE);
            let v: f64 = hzs
                .iter()
                .map(|h| {
                    (1..=5)
                        .map(|k| (2.0 * PI * h * f64::from(k) * t).sin() / f64::from(k))
                        .sum::<f64>()
                })
                .sum();
            (0.08 * v) as f32
        })
        .collect()
}

/// Per-block `(time_ms, root_hz)` of tracker plus stage over `input`, tracking on, manual root
/// `manual`.
fn roots(input: &[f32], manual: f32) -> Vec<(f64, f32)> {
    let mut tracker = PitchTracker::new(RATE, &PitchParams::default());
    let mut stage = RootFollow::new(RATE);
    stage.set_params(&RootFollowParams {
        tracking: true,
        manual_root_hz: manual,
        ..RootFollowParams::default()
    });
    input
        .chunks_exact(BLOCK)
        .enumerate()
        .map(|(i, b)| {
            tracker.process_block(b);
            (
                (i + 1) as f64 * BLOCK as f64 * 1000.0 / f64::from(RATE),
                stage.process_block(tracker.estimate_hz(), tracker.confidence()),
            )
        })
        .collect()
}

#[test]
fn the_root_follows_a_note_change_within_the_documented_time() {
    let mut x = sine(220.0, 1.0);
    x.extend(sine(330.0, 1.0));
    let r = roots(&x, 110.0);
    let change_ms = 1000.0;
    let first = r
        .iter()
        .find(|(t, hz)| *t > change_ms && cents(*hz, 329.63).abs() < LAND_CENTS)
        .expect("root never reaches the new note");
    let latency = first.0 - change_ms;
    println!("note change to within {LAND_CENTS} cents: {latency} ms");
    assert!(latency <= FOLLOW_MS, "{latency}");
    // Settled on the snapped notes before and after.
    let before = r.iter().rev().find(|(t, _)| *t < change_ms).unwrap().1;
    assert!(cents(before, 220.0).abs() < 2.0, "{before}");
    assert!(cents(r.last().unwrap().1, 329.63).abs() < 2.0);
}

#[test]
fn silence_holds_the_last_root() {
    let mut x = sine(220.0, 1.0);
    x.extend(vec![0.0; RATE as usize]);
    let r = roots(&x, 110.0);
    let at_end_of_note = r.iter().rev().find(|(t, _)| *t < 1000.0).unwrap().1;
    for (t, hz) in r.iter().filter(|(t, _)| *t > 1100.0) {
        assert!((hz - at_end_of_note).abs() < 0.05, "{t} {hz}");
    }
}

#[test]
fn noise_and_a_chord_do_not_move_the_root() {
    for x in [noise(2.0), chord(2.0)] {
        for (t, hz) in roots(&x, 110.0) {
            assert!((hz - 110.0).abs() < 0.01, "{t} {hz}");
        }
    }
}

#[test]
fn the_root_glides_and_stays_in_range() {
    let mut x = sine(110.0, 1.0);
    x.extend(sine(880.0, 1.0));
    let r = roots(&x, 110.0);
    let spec = &RESONATOR_PARAMS[0];
    let mut max_step = 0.0f64;
    for w in r.windows(2) {
        assert!(w[1].1.is_finite() && w[1].1 >= spec.min && w[1].1 <= spec.max);
        max_step = max_step.max(cents(w[1].1, w[0].1).abs());
    }
    // Three octaves in one jump would be 3600 cents; the glide moves a fraction per block.
    println!("largest step per block: {max_step} cents");
    assert!(max_step < 800.0, "{max_step}");
}

fn render(params: &ResonatorParams, roots: &[f32], input: &[f32]) -> Vec<[f32; 2]> {
    let mut bank = Resonator::new(RATE, params);
    let mut out = Vec::new();
    for (blk, &root) in input.chunks_exact(BLOCK).zip(roots) {
        bank.set_params(&ResonatorParams {
            root_hz: root,
            ..params.clone()
        });
        for &s in blk {
            out.push(bank.tick([s, s]));
        }
    }
    out
}

#[test]
fn chain_with_tracking_off_is_bit_identical_to_the_manual_root_and_tracking_on_differs() {
    let mut x = sine(220.0, 1.5);
    x.extend(sine(330.0, 1.5));
    let manual = 110.0;
    let params = ResonatorParams {
        root_hz: manual,
        ..ResonatorParams::default()
    };
    let blocks = x.len() / BLOCK;
    let reference = render(&params, &vec![manual; blocks], &x);

    let mut tracker = PitchTracker::new(RATE, &PitchParams::default());
    let mut stage = RootFollow::new(RATE);
    stage.set_params(&RootFollowParams {
        tracking: false,
        manual_root_hz: manual,
        ..RootFollowParams::default()
    });
    let mut off = Vec::new();
    let mut stage_on = RootFollow::new(RATE);
    stage_on.set_params(&RootFollowParams {
        tracking: true,
        manual_root_hz: manual,
        ..RootFollowParams::default()
    });
    let mut on = Vec::new();
    for b in x.chunks_exact(BLOCK) {
        tracker.process_block(b);
        off.push(stage.process_block(tracker.estimate_hz(), tracker.confidence()));
        on.push(stage_on.process_block(tracker.estimate_hz(), tracker.confidence()));
    }
    let chain_off = render(&params, &off, &x);
    assert!(
        chain_off
            .iter()
            .zip(&reference)
            .all(|(a, b)| a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()),
        "tracking off must equal the manual-root bank bit for bit"
    );
    let chain_on = render(&params, &on, &x);
    assert!(
        chain_on
            .iter()
            .zip(&reference)
            .any(|(a, b)| a[0].to_bits() != b[0].to_bits()),
        "negative control: tracking on must differ"
    );
}
