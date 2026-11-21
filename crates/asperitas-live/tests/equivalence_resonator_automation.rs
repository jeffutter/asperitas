//! The live engine running the resonator matches the offline automation render while the controls
//! move by script.
//!
//! Reference: `Automation::<ResonatorControls>::render` over the whole looped clip on a
//! `Resonator` built with the params of `controls_at(0)`, which is what
//! `asperitas-cli process --processor resonator --automation` runs. Engine side: an
//! `Engine` over the same looped source whose `ControlSlot` is fed by a driver standing in for the
//! knob reader. The driver publishes `controls_at(p)` at every stream position `p` divisible by
//! 32 (ramps are sampled per block, so one publish per breakpoint cannot reproduce them) and also
//! at each breakpoint frame that is off the grid. The engine latches only at positions divisible
//! by 32, so an off-grid publish is overwritten by the boundary publish before it is read, and
//! the change lands at the next boundary, as in the offline render. Host buffer lengths are cut
//! at every publish position, so lengths 1, 31 and 33 exercise calls that end mid-block.
//!
//! Tolerance: bit-identical through `f32::to_bits`, because both sides run the same
//! `asperitas-dsp` code on the same input on one machine. The engine's output is pre-clamp; the
//! clamp to [-1, 1] at the device boundary is outside the comparison. No audio device is opened:
//! only `Engine` is used, never the cpal stream.

use std::path::PathBuf;
use std::sync::Arc;

use asperitas_dsp::automation::Automation;
use asperitas_dsp::processor::Frame;
use asperitas_dsp::resonator_bank::Resonator;
use asperitas_dsp::resonator_controls::ResonatorControls;
use asperitas_live::{
    load_clip, ControlSlot, EffectKind, Engine, LoopSource, Source, BLOCK_FRAMES, SAMPLE_RATE,
};

const TOTAL: usize = 8192;

/// Every control: holds (a control left alone between lines), ramps and steps, at frames that are
/// and are not multiples of 32 (0.0021 s = 101, 0.02 s = 960, 0.0427 s = 2050, 0.0777 s = 3730,
/// 0.1 s = 4800, 0.12 s = 5760, 0.1403 s = 6734). The excitation preset is discrete, so it only
/// steps.
const SCRIPT: &str = "\
0.0021 step root_hz=220 damping=0.3
0.02 step spread=0.2
0.0427 ramp root_hz=330
0.0777 step excitation_preset=2
0.1 ramp damping=0.8 structure=0.6
0.12 ramp spread=1.0
0.1403 step excitation_preset=3 root_hz=110
";

const BREAKPOINT_FRAMES: [usize; 7] = [101, 960, 2050, 3730, 4800, 5760, 6734];

fn clip() -> Vec<Frame> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../audio/instruments/mandolin_chord.wav");
    load_clip(path.to_str().unwrap()).unwrap()
}

fn automation() -> Automation<ResonatorControls> {
    Automation::<ResonatorControls>::parse_for(SCRIPT, SAMPLE_RATE).unwrap()
}

fn looped_input() -> Vec<Frame> {
    let mut src = LoopSource::new(clip());
    let mut input = vec![[0.0; 2]; TOTAL];
    src.fill(&mut input);
    input
}

fn reference(a: &Automation<ResonatorControls>) -> Vec<Frame> {
    let mut frames = looped_input();
    let mut fx = Resonator::new(SAMPLE_RATE as f32, &a.controls_at(0).params());
    a.render(&mut fx, &mut frames);
    frames
}

/// The next stream position after `pos` at which the driver publishes.
fn next_publish(pos: usize) -> usize {
    let boundary = (pos / BLOCK_FRAMES + 1) * BLOCK_FRAMES;
    BREAKPOINT_FRAMES
        .iter()
        .copied()
        .filter(|f| *f > pos)
        .fold(boundary, usize::min)
}

/// Engine side. `host_len` gives the next host buffer length.
fn run_engine(
    a: &Automation<ResonatorControls>,
    mut host_len: impl FnMut() -> usize,
) -> Vec<Frame> {
    let slot = Arc::new(ControlSlot::new(&a.controls_at(0)));
    let mut engine = Engine::with_effect(
        EffectKind::Resonator,
        Source::Loop(LoopSource::new(clip())),
        slot.clone(),
    );
    let mut out: Vec<Frame> = Vec::with_capacity(TOTAL);
    while out.len() < TOTAL {
        let pos = out.len();
        if pos.is_multiple_of(BLOCK_FRAMES) || BREAKPOINT_FRAMES.contains(&pos) {
            slot.write(&a.controls_at(pos as u64));
        }
        let n = host_len().min(TOTAL - pos).min(next_publish(pos) - pos);
        let mut buf = vec![[0.0; 2]; n];
        engine.render(&[], &mut buf);
        out.extend(buf);
    }
    out
}

/// Wrong behaviour for the negative control: the controls at each breakpoint frame applied at that
/// exact frame, rather than at the next block boundary.
fn run_exact(a: &Automation<ResonatorControls>) -> Vec<Frame> {
    let mut frames = looped_input();
    let mut fx = Resonator::new(SAMPLE_RATE as f32, &a.controls_at(0).params());
    let mut pos = 0;
    while pos < TOTAL {
        use asperitas_dsp::processor::Processor;
        if pos.is_multiple_of(BLOCK_FRAMES) || BREAKPOINT_FRAMES.contains(&pos) {
            fx.set_params(&a.controls_at(pos as u64).params());
        }
        let end = next_publish(pos).min(TOTAL);
        fx.process_block(&mut frames[pos..end]);
        pos = end;
    }
    frames
}

fn bits_equal(a: &[Frame], b: &[Frame]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x[0].to_bits() == y[0].to_bits() && x[1].to_bits() == y[1].to_bits())
}

/// Deterministic xorshift, so the random buffer sequence is the same on every run.
struct Xorshift(u64);

impl Xorshift {
    fn next_len(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        1 + (self.0 % 1100) as usize
    }
}

#[test]
fn fixture_mixes_on_and_off_grid_breakpoints() {
    let on = BREAKPOINT_FRAMES
        .iter()
        .filter(|f| *f % BLOCK_FRAMES == 0)
        .count();
    assert!(on >= 1 && on < BREAKPOINT_FRAMES.len());
    // The frames above are the ones the parser derives from the script times.
    let frames: Vec<usize> = SCRIPT
        .lines()
        .map(|l| {
            let s: f64 = l.split_whitespace().next().unwrap().parse().unwrap();
            (s * f64::from(SAMPLE_RATE)).round() as usize
        })
        .collect();
    assert_eq!(frames, BREAKPOINT_FRAMES);
    let want = reference(&automation());
    let flat = {
        let mut frames = looped_input();
        let mut fx = Resonator::new(SAMPLE_RATE as f32, &automation().controls_at(0).params());
        use asperitas_dsp::processor::Processor;
        fx.process_block(&mut frames);
        frames
    };
    assert!(
        !bits_equal(&want, &flat),
        "automation must change the output"
    );
}

#[test]
fn engine_matches_the_offline_render_at_every_host_buffer_size() {
    let a = automation();
    let want = reference(&a);
    for host in [1, 31, 32, 33, 64, 480, 512, 1024] {
        let got = run_engine(&a, || host);
        assert!(bits_equal(&got, &want), "host buffer {host}");
    }
}

#[test]
fn engine_matches_the_offline_render_with_random_host_buffer_lengths() {
    let a = automation();
    let want = reference(&a);
    for seed in [0x9E37_79B9_7F4A_7C15u64, 0xD1B5_4A32_D192_ED03] {
        let mut rng = Xorshift(seed);
        let got = run_engine(&a, || rng.next_len());
        assert!(bits_equal(&got, &want), "seed {seed:#x}");
    }
}

/// Negative control: applying a breakpoint at its exact frame instead of the next 32-frame
/// boundary must give different output, so the comparisons above can fail.
#[test]
fn applying_breakpoints_at_the_exact_frame_differs_from_the_block_grid() {
    let a = automation();
    let want = reference(&a);
    assert!(!bits_equal(&run_exact(&a), &want));
    assert!(!bits_equal(&run_exact(&a), &run_engine(&a, || 33)));
}
