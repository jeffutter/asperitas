//! The live engine running the resonator matches the offline CLI path, whatever the host buffer
//! length.
//!
//! Reference: `Resonator::new(48_000.0, &controls.params()).process_block` over the whole clip,
//! which is what `asperitas-cli process --processor resonator` runs. The engine latches the
//! controls with `set_params` before every 32-frame block; the reference never calls it after
//! construction, so a bit-identical result also shows that re-applying unchanged params is a
//! no-op.
//!
//! Tolerance: bit-identical, compared through `f32::to_bits`. Host and device run the same
//! `asperitas-dsp` code on the same input on one machine, so no float slack is needed. Were a
//! reason found that block versus whole-clip processing cannot be bit-identical, the fallback
//! would be the goldens' 1e-6 absolute, stated here.
//!
//! The engine's output is pre-clamp. The clamp to [-1, 1] at the device boundary is outside the
//! comparison.

use std::path::PathBuf;
use std::sync::Arc;

use asperitas_cli::process::parse_resonator_controls;
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_dsp::resonator_bank::Resonator;
use asperitas_dsp::resonator_controls::ResonatorControls;
use asperitas_live::{
    load_clip, ControlSlot, EffectKind, Engine, LoopSource, Source, BLOCK_FRAMES, SAMPLE_RATE,
};

fn clips() -> Vec<(String, Vec<Frame>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../audio/instruments");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no clips in {}", dir.display());
    paths
        .into_iter()
        .map(|p| {
            let frames = load_clip(p.to_str().unwrap()).unwrap();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                frames,
            )
        })
        .collect()
}

fn control_cases() -> Vec<(&'static str, ResonatorControls)> {
    let real_units = parse_resonator_controls(&["root_hz=220".into(), "damping=0.4".into()])
        .expect("real-unit controls parse");
    let preset = parse_resonator_controls(&["excitation_preset=3".into()]).unwrap();
    vec![
        ("default", ResonatorControls::default()),
        ("root_hz=220 damping=0.4", real_units),
        ("excitation_preset=3", preset),
    ]
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

enum Chunking {
    Fixed(usize),
    Random(u64),
}

fn render_engine(clip: &[Frame], controls: &ResonatorControls, chunking: &Chunking) -> Vec<Frame> {
    let slot = Arc::new(ControlSlot::new(controls));
    let mut engine = Engine::with_effect(
        EffectKind::Resonator,
        Source::Loop(LoopSource::new(clip.to_vec())),
        slot,
    );
    let mut rng = match chunking {
        Chunking::Random(seed) => Some(Xorshift(*seed)),
        Chunking::Fixed(_) => None,
    };
    let mut out = Vec::with_capacity(clip.len());
    while out.len() < clip.len() {
        let want = match (&mut rng, chunking) {
            (Some(r), _) => r.next_len(),
            (None, Chunking::Fixed(n)) => *n,
            _ => unreachable!(),
        };
        let n = want.min(clip.len() - out.len());
        let mut buf = vec![[0.0; 2]; n];
        engine.render(&[], &mut buf);
        out.extend(buf);
    }
    out
}

fn first_difference(got: &[Frame], want: &[Frame]) -> Option<usize> {
    assert_eq!(got.len(), want.len());
    got.iter()
        .zip(want)
        .position(|(g, w)| g[0].to_bits() != w[0].to_bits() || g[1].to_bits() != w[1].to_bits())
}

#[test]
fn engine_matches_cli_process_at_any_host_buffer_size() {
    let chunkings: Vec<(String, Chunking)> = [1usize, 31, 32, 33, 64, 480, 512, 1024]
        .into_iter()
        .map(|n| (format!("fixed {n}"), Chunking::Fixed(n)))
        .chain([(
            "random seed 1".to_string(),
            Chunking::Random(0x9E37_79B9_7F4A_7C15),
        )])
        .collect();

    for (name, clip) in clips() {
        for (label, controls) in control_cases() {
            let mut want = clip.clone();
            Resonator::new(SAMPLE_RATE as f32, &controls.params()).process_block(&mut want);
            assert_ne!(
                want, clip,
                "{name} / {label}: the effect left the clip untouched, the comparison is vacuous"
            );

            for (host, chunking) in &chunkings {
                let got = render_engine(&clip, &controls, chunking);
                if let Some(i) = first_difference(&got, &want) {
                    panic!(
                        "{name} / {label} / host {host}: first differing frame {i}: got {:?}, want {:?}",
                        got[i], want[i]
                    );
                }
            }
        }
    }
}

/// Negative control: latching the controls on a grid other than 32 frames from stream start must
/// give different output once the controls move, so the comparisons above can fail.
#[test]
fn latching_on_the_wrong_grid_differs() {
    let clip = first_clip();
    let from = ResonatorControls::default();
    let to = parse_resonator_controls(&["root_hz=330".into(), "damping=0.2".into()]).unwrap();
    let total = 8192;
    let mut input = vec![[0.0; 2]; total];
    LoopSource::new(clip).fill(&mut input);

    // Controls switch at frame 4000; `grid` is the period at which the effect looks for it.
    let render = |grid: usize| {
        let mut fx = Resonator::new(SAMPLE_RATE as f32, &from.params());
        let mut out = input.clone();
        let mut pos = 0;
        while pos < total {
            let c = if pos >= 4000 { &to } else { &from };
            fx.set_params(&c.params());
            let end = (pos + grid).min(total);
            fx.process_block(&mut out[pos..end]);
            pos = end;
        }
        out
    };
    let on_grid = render(BLOCK_FRAMES);
    let off_grid = render(33);
    assert_ne!(on_grid, off_grid);

    // The engine, fed in 480-frame host buffers with the change published at frame 4000 (the
    // slice ends there), agrees with the 32-frame grid and not with the 33-frame one.
    let slot = Arc::new(ControlSlot::new(&from));
    let mut engine = Engine::with_effect(
        EffectKind::Resonator,
        Source::Loop(LoopSource::new(first_clip())),
        slot.clone(),
    );
    let mut got: Vec<Frame> = Vec::new();
    while got.len() < total {
        if got.len() == 4000 {
            slot.write(&to);
        }
        let limit = if got.len() < 4000 { 4000 } else { total };
        let n = 480.min(limit - got.len());
        let mut buf = vec![[0.0; 2]; n];
        engine.render(&[], &mut buf);
        got.extend(buf);
    }
    assert_eq!(first_difference(&got, &on_grid), None);
    assert!(first_difference(&got, &off_grid).is_some());
}

fn first_clip() -> Vec<Frame> {
    clips().remove(0).1
}
