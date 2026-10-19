//! The `cpal` shell around the engine: device and stream setup, the line-in ring, counters.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use asperitas_dsp::asperitas::Controls;
use asperitas_dsp::processor::Frame;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, Stream, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;

use asperitas_live::ring::{RingAction, RingSync};
use asperitas_live::{ControlSlot, Engine, LoopSource, Source, SAMPLE_RATE};

/// Largest slice rendered at once; a longer host buffer is rendered in several slices so the
/// callback never allocates.
const MAX_SLICE: usize = 8192;

/// Smallest ring that rides out callback jitter, in frames.
const MIN_RING_FRAMES: usize = 4096;

/// Ring floor when the host buffer size is the device default and so unknown until a callback
/// runs; macOS default buffers can reach a few thousand frames.
const DEFAULT_BUFFER_RING_FRAMES: usize = 16384;

#[derive(Default)]
struct Counters {
    stream_errors: AtomicU64,
    /// Output callbacks that found the ring short.
    underruns: AtomicU64,
    /// Input callbacks that found the ring full and dropped samples.
    overruns: AtomicU64,
    /// Output callbacks that dropped queued frames to bring latency back down.
    trims: AtomicU64,
    /// The most recent stream error. Written from a cpal error thread, never from an audio
    /// callback, so a mutex is fine.
    last_error: Mutex<Option<String>>,
}

impl Counters {
    fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|e| e.clone())
    }
}

/// Pick a 48 kHz f32 configuration, preferring the device default's channel count.
fn pick_config(
    direction: &str,
    default: Result<SupportedStreamConfig, cpal::Error>,
    ranges: Vec<SupportedStreamConfigRange>,
) -> Result<SupportedStreamConfig, String> {
    let default = default.map_err(|e| format!("{direction} device has no default config: {e}"))?;
    if default.sample_rate() == SAMPLE_RATE && default.sample_format() == cpal::SampleFormat::F32 {
        return Ok(default);
    }
    ranges
        .into_iter()
        .filter(|r| r.sample_format() == cpal::SampleFormat::F32)
        .find_map(|r| r.try_with_sample_rate(SAMPLE_RATE))
        .ok_or_else(|| {
            format!(
                "{direction} device cannot run at {SAMPLE_RATE} Hz f32 (its default is {} Hz {:?}); \
                 asperitas-live does not resample",
                default.sample_rate(),
                default.sample_format()
            )
        })
}

fn stream_config(chosen: &SupportedStreamConfig, buffer: u32) -> StreamConfig {
    let mut config: StreamConfig = chosen.config();
    if buffer > 0 {
        config.buffer_size = match chosen.buffer_size() {
            SupportedBufferSize::Range { min, max } => BufferSize::Fixed(buffer.clamp(*min, *max)),
            SupportedBufferSize::Unknown => BufferSize::Fixed(buffer),
        };
    }
    config
}

fn error_callback(counters: &Arc<Counters>, direction: &'static str) -> impl FnMut(cpal::Error) {
    let counters = counters.clone();
    move |e| {
        counters.stream_errors.fetch_add(1, Ordering::Relaxed);
        // No printing here: the terminal may be in raw mode, where a stray line corrupts the
        // knob view. The view shows the error and `run` prints it once the terminal is restored.
        if let Ok(mut last) = counters.last_error.lock() {
            *last = Some(format!("{direction} stream error: {e}"));
        }
    }
}

/// Write `frames` into interleaved `data` with `channels` channels, clamped to [-1, 1] as the
/// codec boundary does. Channels 0 and 1 carry left and right; a mono device gets their mean and
/// channels beyond the second stay silent.
fn write_interleaved(frames: &[Frame], data: &mut [f32], channels: usize) {
    for (frame, out) in frames.iter().zip(data.chunks_exact_mut(channels)) {
        let [l, r] = frame.map(|s| s.clamp(-1.0, 1.0));
        out.fill(0.0);
        if channels == 1 {
            out[0] = 0.5 * (l + r);
        } else {
            out[0] = l;
            out[1] = r;
        }
    }
}

/// Start the streams, hand the shared knob slot and a reader of the latest stream error to `wait`
/// (which returns when the session ends), then stop the streams and print the counters and the
/// last stream error.
pub fn run(
    clip: Option<Vec<Frame>>,
    controls: &Controls,
    buffer: u32,
    wait: impl FnOnce(&ControlSlot, &dyn Fn() -> Option<String>) -> Result<(), String>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let out_dev: Device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let out_cfg = pick_config(
        "output",
        out_dev.default_output_config(),
        out_dev
            .supported_output_configs()
            .map_err(|e| format!("cannot query output configs: {e}"))?
            .collect(),
    )?;
    let out_channels = out_cfg.channels() as usize;
    let out_conf = stream_config(&out_cfg, buffer);

    let counters = Arc::new(Counters::default());
    let slot = Arc::new(ControlSlot::new(controls));
    let live = clip.is_none();
    let source = match clip {
        Some(frames) => Source::Loop(LoopSource::new(frames)),
        None => Source::External,
    };
    let mut engine = Engine::new(source, slot.clone());

    let mut scratch_in = vec![[0.0f32; 2]; MAX_SLICE];
    let mut scratch_out = vec![[0.0f32; 2]; MAX_SLICE];
    let mut streams: Vec<Stream> = Vec::new();

    let out_stream = if live {
        let in_dev = host
            .default_input_device()
            .ok_or("no default input device")?;
        let in_cfg = pick_config(
            "input",
            in_dev.default_input_config(),
            in_dev
                .supported_input_configs()
                .map_err(|e| format!("cannot query input configs: {e}"))?
                .collect(),
        )?;
        let in_channels = in_cfg.channels() as usize;
        let in_conf = stream_config(&in_cfg, buffer);
        let capacity = match in_conf.buffer_size {
            BufferSize::Fixed(n) => (8 * n as usize).max(MIN_RING_FRAMES),
            BufferSize::Default => DEFAULT_BUFFER_RING_FRAMES,
        };
        let (mut producer, mut consumer) = HeapRb::<f32>::new(capacity).split();

        let overruns = counters.clone();
        let mut in_mono = vec![0.0f32; MAX_SLICE];
        let in_stream = in_dev
            .build_input_stream(
                in_conf,
                move |data: &[f32], _| {
                    // The first channel is the mono source.
                    let mut dropped = false;
                    for chunk in data.chunks(MAX_SLICE * in_channels) {
                        let n = chunk.len() / in_channels;
                        for (m, c) in in_mono[..n].iter_mut().zip(chunk.chunks_exact(in_channels)) {
                            *m = c[0];
                        }
                        dropped |= producer.push_slice(&in_mono[..n]) < n;
                    }
                    if dropped {
                        overruns.overruns.fetch_add(1, Ordering::Relaxed);
                    }
                },
                error_callback(&counters, "input"),
                None,
            )
            .map_err(|e| format!("cannot open input stream: {e}"))?;
        streams.push(in_stream);

        let underruns = counters.clone();
        let trims = counters.clone();
        let mut sync = RingSync::new(capacity);
        let mut mono = vec![0.0f32; MAX_SLICE];
        out_dev.build_output_stream(
            out_conf,
            move |data: &mut [f32], _| {
                let total = data.len() / out_channels;
                // Hold back until one host buffer is queued, and again after an underrun; drop
                // surplus at the start and whenever the ring fills past its high-water mark, so
                // clock drift does not turn into latency.
                match sync.decide(consumer.occupied_len(), total) {
                    RingAction::Wait => {
                        data.fill(0.0);
                        return;
                    }
                    RingAction::Run => {}
                    RingAction::Trim(surplus) => {
                        consumer.skip(surplus);
                        trims.trims.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let mut short = false;
                for out in data.chunks_mut(MAX_SLICE * out_channels) {
                    let n = out.len() / out_channels;
                    let got = consumer.pop_slice(&mut mono[..n]);
                    if got < n {
                        short = true;
                        mono[got..n].fill(0.0);
                    }
                    for (f, &s) in scratch_in[..n].iter_mut().zip(&mono[..n]) {
                        *f = [s, s];
                    }
                    engine.render(&scratch_in[..n], &mut scratch_out[..n]);
                    write_interleaved(&scratch_out[..n], out, out_channels);
                }
                if short {
                    sync.underrun();
                    underruns.underruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            error_callback(&counters, "output"),
            None,
        )
    } else {
        out_dev.build_output_stream(
            out_conf,
            move |data: &mut [f32], _| {
                for out in data.chunks_mut(MAX_SLICE * out_channels) {
                    let n = out.len() / out_channels;
                    engine.render(&[], &mut scratch_out[..n]);
                    write_interleaved(&scratch_out[..n], out, out_channels);
                }
            },
            error_callback(&counters, "output"),
            None,
        )
    }
    .map_err(|e| format!("cannot open output stream: {e}"))?;
    streams.push(out_stream);

    // Output is the last stream built; start the input first so the ring fills before playback.
    for stream in &streams {
        stream
            .play()
            .map_err(|e| format!("cannot start stream: {e}"))?;
    }
    println!(
        "running at {SAMPLE_RATE} Hz, {}",
        if live { "line-in" } else { "looping clip" }
    );

    let waited = wait(&slot, &|| counters.last_error());
    drop(streams);

    println!(
        "stream errors: {}, underruns: {}, overruns: {}, trims: {}",
        counters.stream_errors.load(Ordering::Relaxed),
        counters.underruns.load(Ordering::Relaxed),
        counters.overruns.load(Ordering::Relaxed),
        counters.trims.load(Ordering::Relaxed),
    );
    if let Some(error) = counters.last_error() {
        eprintln!("asperitas-live: {error}");
    }
    waited?;
    if counters.stream_errors.load(Ordering::Relaxed) > 0 {
        return Err("stream errors were reported".to_string());
    }
    Ok(())
}
