//! The live engine matches the offline CLI path, whatever the host buffer length.
//!
//! Reference: `Asperitas::new(48_000.0, &controls).process_block` over the whole clip, which is
//! what `asperitas-cli process` runs and what `asperitas-measure analyze --excerpt` takes as the
//! reference for the device. Live == CLI here, CLI ~ device on the bench, closes the loop.
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

use asperitas_cli::process::parse_asperitas_controls;
use asperitas_dsp::asperitas::{Asperitas, Controls};
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_live::{load_clip, ControlSlot, Engine, LoopSource, Source, SAMPLE_RATE};

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

fn control_cases() -> Vec<(&'static str, Controls)> {
    let real_units = parse_asperitas_controls(&["cutoff_hz=2000".into(), "gain_db=-6".into()])
        .expect("real-unit controls parse");
    vec![
        ("default", Controls::default()),
        ("cutoff_hz=2000 gain_db=-6", real_units),
        ("knobs 0.2 0.9", Controls { knobs: [0.2, 0.9] }),
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

fn render_engine(clip: &[Frame], controls: &Controls, chunking: &Chunking) -> Vec<Frame> {
    let slot = Arc::new(ControlSlot::new(controls));
    let mut engine = Engine::new(Source::Loop(LoopSource::new(clip.to_vec())), slot);
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
            Asperitas::new(SAMPLE_RATE as f32, &controls).process_block(&mut want);
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
