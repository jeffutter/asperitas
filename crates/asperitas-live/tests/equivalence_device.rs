//! The live engine matches the device code path while the knobs move.
//!
//! Reference: `ReplayFx` in patch mode, the no_std stage the rig firmware runs, driven in
//! 32-frame blocks with `set_knobs` before each block. A knob change requested at frame `f`
//! lands at the first block boundary at or after `f`, which the engine reproduces for any host
//! buffer length by latching only at stream positions divisible by 32.
//!
//! Tolerance: bit-identical through `f32::to_bits`, because both sides run the same
//! `asperitas-dsp` code on the same input on one machine. The engine's output is pre-clamp; the
//! clamp to [-1, 1] at the device boundary is outside the comparison.

use std::path::PathBuf;
use std::sync::Arc;

use asperitas_dsp::asperitas::Controls;
use asperitas_dsp::processor::Frame;
use asperitas_live::{load_clip, ControlSlot, Engine, LoopSource, Source, BLOCK_FRAMES};
use asperitas_stimulus::replay_fx::{FxKind, ReplayFx};

const TOTAL: usize = 8192;

fn clip() -> Vec<Frame> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../audio/instruments/mandolin_chord.wav");
    load_clip(path.to_str().unwrap()).unwrap()
}

fn knobs(a: f32, b: f32) -> Controls {
    Controls { knobs: [a, b] }
}

/// Requested change frames that are and are not multiples of 32.
fn mixed_schedule() -> Vec<(usize, Controls)> {
    vec![
        (100, knobs(0.2, 0.9)),
        (128, knobs(0.7, 0.3)),
        (1000, knobs(0.1, 0.6)),
        (2047, knobs(0.9, 0.2)),
        (2048, knobs(0.4, 0.4)),
    ]
}

fn aligned_schedule() -> Vec<(usize, Controls)> {
    vec![
        (128, knobs(0.2, 0.9)),
        (1024, knobs(0.7, 0.3)),
        (2048, knobs(0.1, 0.6)),
    ]
}

/// Engine side: the host calls with `host`-frame buffers and publishes each change at exactly
/// its requested frame, splitting a call there.
fn run_engine(schedule: &[(usize, Controls)], host: usize) -> Vec<Frame> {
    let slot = Arc::new(ControlSlot::new(&Controls::default()));
    let mut engine = Engine::new(Source::Loop(LoopSource::new(clip())), slot.clone());
    let mut out: Vec<Frame> = Vec::with_capacity(TOTAL);
    let mut pending = schedule.iter().peekable();
    while out.len() < TOTAL {
        while let Some((f, c)) = pending.peek() {
            if *f <= out.len() {
                slot.write(c);
                pending.next();
            } else {
                break;
            }
        }
        let mut n = host.min(TOTAL - out.len());
        if let Some((f, _)) = pending.peek() {
            n = n.min(f - out.len());
        }
        let mut buf = vec![[0.0; 2]; n];
        engine.render(&[], &mut buf);
        out.extend(buf);
    }
    out
}

/// Device side: 32-frame blocks; the change lands at the first boundary at or after `f`, or,
/// when `exact`, at the requested frame itself (the wrong behaviour, for the negative control).
fn run_reference(schedule: &[(usize, Controls)], exact: bool) -> Vec<Frame> {
    let mut fx = ReplayFx::new();
    fx.select(FxKind::Patch, Controls::default().knobs);
    let mut src = LoopSource::new(clip());
    let mut out: Vec<Frame> = Vec::with_capacity(TOTAL);
    let mut pending = schedule.iter().peekable();
    while out.len() < TOTAL {
        let pos = out.len();
        let mut n = BLOCK_FRAMES.min(TOTAL - pos);
        if pos.is_multiple_of(BLOCK_FRAMES) {
            while let Some((f, c)) = pending.peek() {
                if *f <= pos {
                    fx.set_knobs(c.knobs);
                    pending.next();
                } else {
                    break;
                }
            }
        }
        if exact {
            if let Some((f, c)) = pending.peek() {
                if *f < pos + n {
                    n = f - pos;
                    if n == 0 {
                        fx.set_knobs(c.knobs);
                        pending.next();
                        continue;
                    }
                }
            }
        }
        let mut block = vec![[0.0; 2]; n];
        src.fill(&mut block);
        fx.process(&mut block);
        out.extend(block);
    }
    out
}

fn bits_equal(a: &[Frame], b: &[Frame]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x[0].to_bits() == y[0].to_bits() && x[1].to_bits() == y[1].to_bits())
}

const HOSTS: [usize; 7] = [1, 31, 32, 33, 64, 480, 1024];

#[test]
fn engine_matches_replay_fx_with_knob_changes_off_and_on_the_block_grid() {
    let schedule = mixed_schedule();
    let want = run_reference(&schedule, false);
    for host in HOSTS {
        let got = run_engine(&schedule, host);
        assert!(bits_equal(&got, &want), "host buffer {host}");
    }
}

#[test]
fn engine_matches_replay_fx_with_block_aligned_changes() {
    let schedule = aligned_schedule();
    let want = run_reference(&schedule, false);
    for host in HOSTS {
        let got = run_engine(&schedule, host);
        assert!(bits_equal(&got, &want), "host buffer {host}");
    }
    // Sanity for the negative control: where every change is already on the grid, applying it
    // at the exact frame is the same thing.
    assert!(bits_equal(&run_reference(&schedule, true), &want));
}

/// Negative control: applying a change at the exact requested frame instead of the next 32-frame
/// boundary must give different output, so the comparison above can fail.
#[test]
fn applying_changes_at_the_exact_frame_differs_from_the_block_grid() {
    let schedule = mixed_schedule();
    let grid = run_reference(&schedule, false);
    let exact = run_reference(&schedule, true);
    assert!(!bits_equal(&exact, &grid));
    let engine = run_engine(&schedule, 33);
    assert!(!bits_equal(&engine, &exact));
}
