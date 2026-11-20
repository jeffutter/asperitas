//! Host cost of the resonator bank per 32-frame block.
//!
//! `cargo run --release -p asperitas-dsp --example bank_cost`
//!
//! Prints microseconds per block and as a share of the 667 us block (32 frames at 48 kHz). This
//! is a host measurement, not a Cortex-M7 one. The worst case moves the root, structure and
//! damping every block so every voice recomputes its coefficients (exp, sin, cos) on every sample.

use asperitas_dsp::{Frame, Processor, Resonator, ResonatorParams};
use std::hint::black_box;
use std::time::Instant;

const BLOCK: usize = 32;
const BLOCK_US: f64 = 32.0 / 48_000.0 * 1e6;
const BLOCKS: usize = 200_000;

fn run(name: &str, vary: bool) {
    let mut r = Resonator::new(48_000.0, &ResonatorParams::default());
    let mut block = [[0.0f32; 2]; BLOCK];
    let mut p = ResonatorParams::default();
    let mut seed = 1u32;
    let start = Instant::now();
    for b in 0..BLOCKS {
        for f in block.iter_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = 0.3 * ((seed >> 8) as f32 / (1 << 23) as f32 - 1.0);
            *f = [v, v] as Frame;
        }
        if vary {
            let t = (b % 1000) as f32 / 1000.0;
            p.root_hz = 80.0 + 60.0 * t;
            p.structure = t;
            p.damping = 0.3 + 0.4 * t;
            p.spread = 1.0 - t;
            r.set_params(&p);
        }
        r.process_block(&mut block);
        black_box(&block);
    }
    let us = start.elapsed().as_secs_f64() * 1e6 / BLOCKS as f64;
    println!(
        "{name}: {us:.2} us per block, {:.1} % of the {BLOCK_US:.0} us block",
        100.0 * us / BLOCK_US
    );
}

fn main() {
    run("settled params (default voicing)", false);
    run("moving params (every coefficient recomputed)", true);
}
