# asperitas-live: the effect in real time on the desktop

`asperitas-live` (crate `crates/asperitas-live`) runs `asperitas_dsp::asperitas::Asperitas` on the
default audio devices through `cpal`, so sound design needs no flash cycle. It plays a looping WAV
(the corpus clips in `audio/instruments`) or live line-in from the default input device.

## Usage

```
cargo run -p asperitas-live --release -- --input audio/instruments/mandolin_chord.wav --params cutoff_hz=2000
cargo run -p asperitas-live --release -- --line-in --params cutoff_hz=800 gain_db=-6 --buffer 64
```

| Flag | Meaning |
|---|---|
| `--input <wav>` | 48 kHz WAV clip, played in a loop. Exactly one of `--input` and `--line-in` is required. |
| `--line-in` | Use the default input device (first channel, treated as mono). macOS asks the terminal for microphone permission on first use. Use headphones: a microphone next to speakers feeds back. |
| `--params k=v ...` | Knob values in real units, the same syntax and translation as `asperitas-cli process` (`cutoff_hz`, `gain_db`). Fixed for the run. |
| `--buffer <frames>` | Host buffer size, default 128; `0` takes the device default. Clamped to the device's range. |
| `--seconds <n>` | Stop after `n` seconds. Without it the stream runs until Enter is pressed. |

On exit it prints the stream error count and, for line-in, the ring underrun, overrun and trim counts. Each counts callbacks (events), not samples: an underrun is an output callback that found the ring short, an overrun an input callback that had to drop samples, a trim an output callback that dropped queued frames.
It exits non-zero when a stream reported an error.

## Behaviour

- The sample rate is 48 kHz and nothing resamples. A WAV at another rate, or a device that cannot
  open at 48 kHz f32, is refused with a message naming the rate.
- A mono source feeds both channels of the stereo frame, as on the device. Output channels beyond
  the second are silent; a mono output device receives the mean of left and right.
- Output is clamped to [-1, 1] at the device boundary, as the firmware codec boundary does; the
  engine's own output is unclamped.
- Controls are latched only at stream positions divisible by 32, as the firmware calls
  `set_controls` once per 32-frame block, whatever the host buffer length. Decision and evidence:
  [cpal-macos-duplex.md](../investigations/cpal-macos-duplex.md).
- Line-in uses two independent `cpal` streams joined by a lock-free ring; the output side waits
  until one host buffer is queued and trims a large surplus down to a margin above it. It waits
  again after an underrun, and trims back to a margin above one host buffer (halfway to the
  high-water mark) whenever the ring fills past half its capacity (at least two host buffers), so clock drift between the streams does not stick as silence or as
  latency. `--buffer 0` sizes the ring at 16384 frames since the host buffer is unknown up front.
