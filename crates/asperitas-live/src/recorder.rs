//! `--record`: write what the engine hands to the audio device into a WAV file.
//!
//! The audio callback must not allocate or touch the disk, so it only pushes into a lock-free
//! ring ([`RecordTap`]) and a writer thread drains the ring into the file. When the writer falls
//! behind the ring fills and frames are dropped and counted; [`Recorder::finish`] reports the
//! count so a recording with gaps is never mistaken for a clean one.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use asperitas_dsp::processor::Frame;
use asperitas_live::SAMPLE_RATE;
use hound::{SampleFormat, WavSpec, WavWriter};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;

/// Ring size in samples: two seconds of stereo, enough to ride out a slow disk write.
const RING_SAMPLES: usize = 2 * SAMPLE_RATE as usize * 2;

/// How long the writer sleeps when the ring is empty.
const WRITER_IDLE: Duration = Duration::from_millis(5);

/// The audio-callback side: push the frames about to be played.
pub struct RecordTap {
    producer: ringbuf::HeapProd<f32>,
    dropped: Arc<AtomicU64>,
}

impl RecordTap {
    /// Queue `frames` for the writer, clamped to [-1, 1] as the device boundary does. Wait-free;
    /// frames that do not fit are dropped and counted.
    pub fn push(&mut self, frames: &[Frame]) {
        let mut dropped = 0;
        for frame in frames {
            if self.producer.vacant_len() < 2 {
                dropped += 1;
                continue;
            }
            self.producer.push_slice(&frame.map(|s| s.clamp(-1.0, 1.0)));
        }
        if dropped > 0 {
            self.dropped.fetch_add(dropped, Ordering::Relaxed);
        }
    }
}

/// What a finished recording holds.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub frames_written: u64,
    pub frames_dropped: u64,
}

/// The control side: owns the writer thread.
pub struct Recorder {
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    writer: JoinHandle<Result<u64, String>>,
}

impl Recorder {
    /// Create `path` (a 48 kHz stereo 32-bit float WAV) and start the writer. The file is created
    /// here, before any stream starts, so a bad path fails the run up front.
    pub fn start(path: &Path) -> Result<(Recorder, RecordTap), String> {
        let spec = WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut wav = WavWriter::create(path, spec)
            .map_err(|e| format!("cannot create '{}': {e}", path.display()))?;
        let (producer, mut consumer) = HeapRb::<f32>::new(RING_SAMPLES).split();
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));

        let stop_flag = stop.clone();
        let writer = std::thread::spawn(move || -> Result<u64, String> {
            let mut samples: u64 = 0;
            let mut chunk = vec![0.0f32; 4096];
            loop {
                // Read the flag before draining: whatever the producer pushed before it set the
                // flag is then guaranteed to be in this drain.
                let stopping = stop_flag.load(Ordering::Acquire);
                let n = consumer.pop_slice(&mut chunk);
                for &s in &chunk[..n] {
                    wav.write_sample(s)
                        .map_err(|e| format!("write failed: {e}"))?;
                }
                samples += n as u64;
                if n == 0 {
                    if stopping {
                        break;
                    }
                    std::thread::sleep(WRITER_IDLE);
                }
            }
            wav.finalize()
                .map_err(|e| format!("cannot finish the WAV: {e}"))?;
            Ok(samples / 2)
        });

        Ok((
            Recorder {
                stop,
                dropped: dropped.clone(),
                writer,
            },
            RecordTap { producer, dropped },
        ))
    }

    /// Flush what is queued, close the file and report it. The tap must no longer be pushing
    /// (the streams are dropped first), or its last frames may be missed.
    pub fn finish(self) -> Result<Summary, String> {
        self.stop.store(true, Ordering::Release);
        let frames_written = self
            .writer
            .join()
            .map_err(|_| "the recording writer panicked".to_string())??;
        Ok(Summary {
            frames_written,
            frames_dropped: self.dropped.load(Ordering::Relaxed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_clamped_stereo_frames_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.wav");
        let (recorder, mut tap) = Recorder::start(&path).unwrap();
        tap.push(&[[0.25, -0.5], [2.0, -3.0]]);
        tap.push(&[[0.0, 1.0]]);
        let summary = recorder.finish().unwrap();
        assert_eq!(
            summary,
            Summary {
                frames_written: 3,
                frames_dropped: 0
            }
        );

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.sample_format),
            (2, SAMPLE_RATE, SampleFormat::Float)
        );
        let samples: Vec<f32> = reader.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(samples, [0.25, -0.5, 1.0, -1.0, 0.0, 1.0]);
    }

    #[test]
    fn bad_path_fails_at_start() {
        let err = Recorder::start(Path::new("/nonexistent-dir/out.wav")).err();
        assert!(err.is_some_and(|e| e.contains("cannot create")));
    }
}
