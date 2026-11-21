//! Host cost of the pitch tracker per 32-frame block.
//!
//! `cargo run --release -p asperitas-dsp --features pitch --example pitch_cost`
//!
//! Prints the average, p90, p99.9 and maximum block time in microseconds against the 667 us
//! block (32 frames at 48 kHz). The slow blocks are the ones that run an analysis (one in four). This is a host measurement, not a Cortex-M7 one.

use asperitas_dsp::{PitchParams, PitchTracker};
use std::hint::black_box;
use std::time::Instant;

const BLOCK: usize = 32;
const BLOCK_US: f64 = 32.0 / 48_000.0 * 1e6;
const BLOCKS: usize = 100_000;

fn run(name: &str, mut signal: impl FnMut(usize) -> f32) {
    let mut t = PitchTracker::new(48_000.0, &PitchParams::default());
    let mut block = [0.0f32; BLOCK];
    let mut times = Vec::with_capacity(BLOCKS);
    for b in 0..BLOCKS {
        for (i, s) in block.iter_mut().enumerate() {
            *s = signal(b * BLOCK + i);
        }
        let start = Instant::now();
        t.process_block(black_box(&block));
        let us = start.elapsed().as_secs_f64() * 1e6;
        black_box(t.estimate_hz());
        times.push(us);
    }
    let avg = times.iter().sum::<f64>() / BLOCKS as f64;
    times.sort_by(|a, b| a.total_cmp(b));
    // One block in four runs an analysis, so p90 is a typical analysis block; p99.9 and the
    // maximum include host scheduling jitter.
    let at = |q: f64| times[((BLOCKS - 1) as f64 * q) as usize];
    println!(
        "{name}: avg {avg:.2} us, p90 {:.2} us ({:.1} %), p99.9 {:.2} us, max {:.2} us, of the {BLOCK_US:.0} us block",
        at(0.9),
        100.0 * at(0.9) / BLOCK_US,
        at(0.999),
        times[BLOCKS - 1],
    );
}

fn main() {
    let two_pi = 2.0 * std::f32::consts::PI;
    run("41 Hz sine", |n| {
        0.5 * (two_pi * 41.0 * n as f32 / 48_000.0).sin()
    });
    run("1760 Hz sine", |n| {
        0.5 * (two_pi * 1760.0 * n as f32 / 48_000.0).sin()
    });
    let mut seed = 1u32;
    run("white noise", move |_| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        0.5 * ((seed >> 8) as f32 / (1 << 23) as f32 - 1.0)
    });
    run("silence", |_| 0.0);
}
