//! The processing core: a source, the effect, and the block-grid control latch.

use std::sync::Arc;

use asperitas_cli::wav_io::read_wav;
use asperitas_dsp::asperitas::{Asperitas, Controls};
use asperitas_dsp::processor::{Frame, Processor};

use crate::controls::ControlSlot;

/// The effect's sample rate. The device runs the effect at this rate and nothing resamples.
pub const SAMPLE_RATE: u32 = 48_000;

/// Frames per control block on the device. The firmware applies its knob reading once before
/// each block of this length; the engine reproduces that timing for any host buffer length by
/// latching controls only at frame indices divisible by this, counted from stream start.
pub const BLOCK_FRAMES: usize = 32;

/// Read a WAV clip for looping. Refuses a clip that is not at [`SAMPLE_RATE`] or has no frames.
pub fn load_clip(path: &str) -> Result<Vec<Frame>, String> {
    let (spec, frames) = read_wav(path)?;
    if spec.sample_rate != SAMPLE_RATE {
        return Err(format!(
            "'{path}' is {} Hz; asperitas-live runs at {SAMPLE_RATE} Hz and does not resample",
            spec.sample_rate
        ));
    }
    if frames.is_empty() {
        return Err(format!("'{path}' contains no audio frames"));
    }
    Ok(frames)
}

/// A clip played in a loop. Wrapping is sample-exact: the frame after the last is the first.
pub struct LoopSource {
    frames: Vec<Frame>,
    pos: usize,
}

impl LoopSource {
    /// Loop over `frames`. Panics on an empty clip, which has nothing to play.
    pub fn new(frames: Vec<Frame>) -> Self {
        assert!(!frames.is_empty(), "a loop needs at least one frame");
        Self { frames, pos: 0 }
    }

    /// Fill `out` with the next frames of the loop, wrapping as often as needed.
    pub fn fill(&mut self, out: &mut [Frame]) {
        let mut done = 0;
        while done < out.len() {
            let n = (self.frames.len() - self.pos).min(out.len() - done);
            out[done..done + n].copy_from_slice(&self.frames[self.pos..self.pos + n]);
            self.pos = (self.pos + n) % self.frames.len();
            done += n;
        }
    }
}

/// Where the engine's input frames come from.
pub enum Source {
    /// A looping clip.
    Loop(LoopSource),
    /// Frames supplied by the caller on each [`Engine::render`] call (live line-in).
    External,
}

/// Source, effect and control latch. Output is pre-clamp: the clamp to [-1, 1] belongs to the
/// device boundary, not to the processing.
pub struct Engine {
    effect: Asperitas,
    source: Source,
    slot: Arc<ControlSlot>,
    /// Frames rendered since stream start.
    frame: u64,
}

impl Engine {
    /// An engine whose effect starts with the slot's current controls already in force.
    pub fn new(source: Source, slot: Arc<ControlSlot>) -> Self {
        let effect = Asperitas::new(SAMPLE_RATE as f32, &slot.read());
        Self {
            effect,
            source,
            slot,
            frame: 0,
        }
    }

    /// Frames rendered since stream start.
    pub fn frames_rendered(&self) -> u64 {
        self.frame
    }

    /// Render `out.len()` frames. `external` supplies the input for [`Source::External`] and must
    /// be as long as `out`; a looping source ignores it. Whatever the call length, controls are
    /// read from the slot only when the stream position is divisible by [`BLOCK_FRAMES`], so a
    /// change published at any moment lands at the next such position. Never allocates.
    pub fn render(&mut self, external: &[Frame], out: &mut [Frame]) {
        let mut done = 0;
        while done < out.len() {
            let into_block = (self.frame % BLOCK_FRAMES as u64) as usize;
            if into_block == 0 {
                let controls: Controls = self.slot.read();
                self.effect.set_controls(&controls);
            }
            let n = (BLOCK_FRAMES - into_block).min(out.len() - done);
            let segment = &mut out[done..done + n];
            match &mut self.source {
                Source::Loop(clip) => clip.fill(segment),
                Source::External => segment.copy_from_slice(&external[done..done + n]),
            }
            self.effect.process_block(segment);
            self.frame += n as u64;
            done += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<Frame> {
        (0..n)
            .map(|i| [i as f32 / n as f32, -(i as f32) / n as f32])
            .collect()
    }

    #[test]
    fn loop_wraps_sample_exactly_at_any_chunking() {
        let clip = ramp(100);
        for chunk in [1usize, 7, 31, 32, 33, 99, 100, 101, 480, 512] {
            let mut src = LoopSource::new(clip.clone());
            let mut got = Vec::new();
            let mut buf = vec![[0.0; 2]; chunk];
            while got.len() < 1000 {
                src.fill(&mut buf);
                got.extend_from_slice(&buf);
            }
            for (i, f) in got.iter().enumerate() {
                assert_eq!(*f, clip[i % 100], "chunk {chunk}, frame {i}");
            }
        }
    }

    /// The device: 32-frame blocks, controls read before each block.
    fn device_reference(
        clip: &[Frame],
        frames: usize,
        change_at: usize,
        to: &Controls,
    ) -> Vec<Frame> {
        let mut fx = Asperitas::new(SAMPLE_RATE as f32, &Controls::default());
        let mut src = LoopSource::new(clip.to_vec());
        let mut out = Vec::new();
        for start in (0..frames).step_by(BLOCK_FRAMES) {
            let c = if start >= change_at {
                to.clone()
            } else {
                Controls::default()
            };
            fx.set_controls(&c);
            let mut block = vec![[0.0; 2]; BLOCK_FRAMES.min(frames - start)];
            src.fill(&mut block);
            fx.process_block(&mut block);
            out.extend(block);
        }
        out
    }

    #[test]
    fn control_change_lands_on_next_block_boundary_for_any_buffer_length() {
        let clip = ramp(300);
        let to = Controls { knobs: [0.2, 0.9] };
        let total = 2048;
        for host in [1usize, 31, 32, 33, 480, 512] {
            // Publish the change after the first host buffer past frame 100.
            let slot = Arc::new(ControlSlot::new(&Controls::default()));
            let mut engine = Engine::new(Source::Loop(LoopSource::new(clip.clone())), slot.clone());
            let mut got: Vec<Frame> = Vec::new();
            let mut change_at = None;
            while got.len() < total {
                if change_at.is_none() && got.len() >= 100 {
                    slot.write(&to);
                    change_at = Some(got.len().div_ceil(BLOCK_FRAMES) * BLOCK_FRAMES);
                }
                let n = host.min(total - got.len());
                let mut buf = vec![[0.0; 2]; n];
                engine.render(&[], &mut buf);
                got.extend(buf);
            }
            let want = device_reference(&clip, total, change_at.unwrap(), &to);
            assert_eq!(got, want, "host buffer {host}");
        }
    }

    #[test]
    fn external_source_passes_input_through_the_effect() {
        let slot = Arc::new(ControlSlot::new(&Controls::default()));
        let mut engine = Engine::new(Source::External, slot);
        let input = ramp(64);
        let mut out = vec![[0.0; 2]; 64];
        engine.render(&input, &mut out);
        let mut fx = Asperitas::new(SAMPLE_RATE as f32, &Controls::default());
        let mut want = input.clone();
        fx.set_controls(&Controls::default());
        fx.process_block(&mut want[..32]);
        fx.set_controls(&Controls::default());
        fx.process_block(&mut want[32..]);
        assert_eq!(out, want);
    }

    #[test]
    fn load_clip_refuses_other_rates_naming_the_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        let path = path.to_str().unwrap();
        asperitas_cli::wav_io::write_wav(path, 44_100, &ramp(10)).unwrap();
        let err = load_clip(path).unwrap_err();
        assert!(err.contains("44100 Hz"), "{err}");
        assert!(err.contains("48000"), "{err}");
    }

    #[test]
    fn load_clip_accepts_48k() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        let path = path.to_str().unwrap();
        asperitas_cli::wav_io::write_wav(path, 48_000, &ramp(10)).unwrap();
        assert_eq!(load_clip(path).unwrap().len(), 10);
    }
}
