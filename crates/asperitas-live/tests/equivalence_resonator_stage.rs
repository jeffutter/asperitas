//! The rig's resonator stage computes what `asperitas-cli process --processor resonator` computes.
//!
//! Reference: the CLI's own `run_process` over a WAV of the clip, with `--params` or
//! `--automation`, read back through `wav_io` (32-bit float, so lossless). Stage side:
//! `ReplayFx` and `FxChain`, the no_std code the rig firmware runs, driven in 32-frame blocks with
//! the knob positions of `Automation::controls_at` at each block start applied before the block,
//! the way the rig's override merge does. Both are asserted bit for bit through `f32::to_bits`:
//! they run the same `asperitas-dsp` code on the same input on one machine.
//!
//! What the wire adds is out of scope here: it carries each level in 16 bits, so a device render
//! differs from this one by at most one wire step on a non-default position. The stage itself is
//! what this file pins.
//!
//! Two stage setups are compared. `ReplayFx::select` with the script's starting positions is the
//! stage proper. `FxChain` (the `CTL FX` path: select by wire code, then the override merge) starts
//! the stage at the resonator's default positions, so its script begins at the defaults.

use std::path::{Path, PathBuf};

use asperitas_cli::process::{run_process, ProcessArgs};
use asperitas_cli::wav_io::{read_wav, write_wav};
use asperitas_dsp::automation::Automation;
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_dsp::resonator_controls::ResonatorControls;
use asperitas_stimulus::fx_chain::FxChain;
use asperitas_stimulus::replay_fx::{FxKind, ReplayFx};

const RATE: u32 = 48_000;
const BLOCK: usize = 32;
const TOTAL: usize = 16_384;

/// Every control moves, at frames on and off the 32-frame grid. Frame 0 names non-default values.
const SCRIPT: &str = "\
0 root_hz=196 structure=0.2 damping=0.4 spread=0.5
0.0021 step damping=0.3
0.02 step spread=0.2
0.0427 ramp root_hz=330
0.0777 step excitation_preset=2
0.1 ramp damping=0.8 structure=0.6
0.12 ramp spread=1.0
0.1403 step excitation_preset=3 root_hz=110
";

/// As [`SCRIPT`], but every control is at its default at frame 0, so a stage selected by wire code
/// (which starts at the defaults) begins where the offline render does.
const DEFAULT_START_SCRIPT: &str = "\
0.0021 step root_hz=220 damping=0.3
0.02 step spread=0.2
0.0427 ramp root_hz=330
0.0777 step excitation_preset=2
0.1 ramp damping=0.8 structure=0.6
0.12 ramp spread=1.0
0.1403 step excitation_preset=3 root_hz=110
";

fn clip() -> Vec<Frame> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../audio/instruments/mandolin_chord.wav");
    let (_, mut frames) = read_wav(path.to_str().unwrap()).unwrap();
    frames.truncate(TOTAL);
    assert_eq!(frames.len(), TOTAL, "the fixture clip is long enough");
    frames
}

/// Plays a clip as the generator in front of the stage, as the rig's excerpt replay does.
struct ClipGen {
    clip: Vec<Frame>,
    pos: usize,
}

impl Processor for ClipGen {
    type Params = ();
    fn set_sample_rate(&mut self, _hz: f32) {}
    fn set_params(&mut self, _params: &()) {}
    fn tick(&mut self, _input: Frame) -> Frame {
        let f = self.clip[self.pos];
        self.pos += 1;
        f
    }
    fn reset(&mut self) {
        self.pos = 0;
    }
}

/// The CLI's output for `clip` with `extra` arguments, computed through its library entry point.
fn cli_render(dir: &Path, clip: &[Frame], params: Vec<String>, script: Option<&str>) -> Vec<Frame> {
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_wav(input.to_str().unwrap(), RATE, clip).unwrap();
    let automation = script.map(|text| {
        let path = dir.join("knobs.txt");
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_string()
    });
    run_process(&ProcessArgs {
        input_path: input.to_str().unwrap().into(),
        output_path: output.to_str().unwrap().into(),
        processor_name: "resonator".into(),
        params,
        automation,
    })
    .unwrap();
    read_wav(output.to_str().unwrap()).unwrap().1
}

fn automation(script: &str) -> Automation<ResonatorControls> {
    Automation::<ResonatorControls>::parse_for(script, RATE).unwrap()
}

/// The stage proper: `ReplayFx`, selected at the script's starting positions, then each block's
/// positions applied `lag` blocks late (0 is correct; 1 is the negative control).
fn stage_render(clip: &[Frame], a: &Automation<ResonatorControls>, lag: usize) -> Vec<Frame> {
    let mut fx = ReplayFx::new();
    fx.select(FxKind::Resonator, a.controls_at(0).positions);
    let mut out = clip.to_vec();
    for (block, chunk) in out.chunks_mut(BLOCK).enumerate() {
        let at = block.saturating_sub(lag) * BLOCK;
        fx.set_knobs(a.controls_at(at as u64).positions);
        fx.process(chunk);
    }
    out
}

/// The `CTL FX` path: a chain over the clip, selected by wire code 2, its knobs rewritten by the
/// merge closure each block as the firmware's override merge does.
fn chain_render(clip: &[Frame], a: &Automation<ResonatorControls>) -> Vec<Frame> {
    let mut chain = FxChain::new(ClipGen {
        clip: clip.to_vec(),
        pos: 0,
    });
    chain.set_sample_rate(RATE as f32);
    assert!(chain.select(2, 0));
    let mut out = Vec::with_capacity(clip.len());
    for block in 0..clip.len() / BLOCK {
        let mut frames = [[0.0; 2]; BLOCK];
        let positions = a.controls_at((block * BLOCK) as u64).positions;
        chain.process_block_with(&mut frames, |knobs| knobs.copy_from_slice(&positions));
        out.extend_from_slice(&frames);
    }
    out
}

fn bits_equal(a: &[Frame], b: &[Frame]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x[0].to_bits() == y[0].to_bits() && x[1].to_bits() == y[1].to_bits())
}

#[test]
fn the_stage_matches_the_cli_under_an_automation_file() {
    let dir = tempfile::tempdir().unwrap();
    let clip = clip();
    let want = cli_render(dir.path(), &clip, vec![], Some(SCRIPT));
    let got = stage_render(&clip, &automation(SCRIPT), 0);
    assert!(bits_equal(&got, &want));
}

#[test]
fn the_wire_selected_chain_matches_the_cli_under_an_automation_file() {
    let dir = tempfile::tempdir().unwrap();
    let clip = clip();
    let want = cli_render(dir.path(), &clip, vec![], Some(DEFAULT_START_SCRIPT));
    let got = chain_render(&clip, &automation(DEFAULT_START_SCRIPT));
    assert!(bits_equal(&got, &want));
}

#[test]
fn the_stage_matches_the_cli_at_fixed_real_unit_params() {
    let dir = tempfile::tempdir().unwrap();
    let clip = clip();
    let params = ["root_hz=330", "structure=0.4", "damping=0.6", "spread=0.7"];
    let want = cli_render(
        dir.path(),
        &clip,
        params.iter().map(|p| p.to_string()).collect(),
        None,
    );
    let values: Vec<(&str, f32)> = params
        .iter()
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (k, v.parse().unwrap())
        })
        .collect();
    let positions = ResonatorControls::from_values(&values).unwrap().positions;
    let mut fx = ReplayFx::new();
    fx.select(FxKind::Resonator, positions);
    let mut got = clip.clone();
    for chunk in got.chunks_mut(BLOCK) {
        fx.set_knobs(positions);
        fx.process(chunk);
    }
    assert!(bits_equal(&got, &want));
}

/// Negative controls: the comparisons above can fail. A knob schedule applied one block late, a
/// different script, and the bypass stage each give different output from the CLI's render.
#[test]
fn a_late_a_different_or_a_bypassed_stage_differs_from_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let clip = clip();
    let want = cli_render(dir.path(), &clip, vec![], Some(SCRIPT));
    let a = automation(SCRIPT);
    assert!(
        !bits_equal(&stage_render(&clip, &a, 1), &want),
        "one block late"
    );
    let other = SCRIPT.replace("0.0021 step damping=0.3", "0.0021 step damping=0.31");
    assert!(
        !bits_equal(&stage_render(&clip, &automation(&other), 0), &want),
        "a different script"
    );
    let mut bypass = ReplayFx::new();
    let mut passed = clip.clone();
    bypass.process(&mut passed);
    assert!(!bits_equal(&passed, &want), "bypass");
    // Bypass is the default and returns the input bit for bit.
    assert!(bits_equal(&passed, &clip));
}
