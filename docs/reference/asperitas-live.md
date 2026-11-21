# asperitas-live: the effect in real time on the desktop

`asperitas-live` (crate `crates/asperitas-live`) runs `asperitas_dsp::asperitas::Asperitas` (or, with
`--effect resonator`, the resonator bank) on the default audio devices through `cpal`, so sound design needs no flash cycle. It plays a looping WAV
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
| `--effect <asperitas\|resonator>` | The effect to run; default `asperitas`, so no selector runs the low-pass and gain patch as before. |
| `--params k=v ...` | Starting knob values in real units, the same syntax and translation as `asperitas-cli process` for the selected effect (`cutoff_hz`, `gain_db` for `asperitas`; the keys below for `resonator`). The knob view moves them afterwards. A key of the other effect is an error naming the key and the effect. |
| `--buffer <frames>` | Host buffer size, default 128; `0` takes the device default. Clamped to the device's range. |
| `--record <wav>` | Also write what is sent to the output device, after the clamp, to a 48 kHz stereo 32-bit float WAV (overwritten). Works with `--input` and `--line-in`. |
| `--seconds <n>` | Stop after `n` seconds. Without it the stream runs until esc (knob view) or Enter (no terminal). |

## Resonator

```
cargo run -p asperitas-live --release -- --effect resonator --input audio/instruments/mandolin_chord.wav --params root_hz=220 damping=0.4
```

`--params` takes the resonator's real-unit keys, parsed and range-checked by the same code as
`asperitas-cli process --processor resonator` (ranges and meanings in
[resonator.md](resonator.md)): `root_hz`, `structure`, `damping`, `spread`, and `excitation_preset`
(a whole number, 1 to 3). `--record`, `--buffer`, `--seconds` and `--line-in` work as for the default
effect.

Equivalence guarantee: for the same controls and input, the live engine's output is bit-for-bit
the output of `asperitas-cli process --processor resonator`, at every host buffer size, including
sizes that are not a multiple of 32. Controls are latched with `set_params` only at stream positions
divisible by 32, counted from stream start, and the same holds with an automation file driving the
controls (`--automation` on the CLI; the live tests drive the slot from the same script).
Re-applying unchanged params every block leaves the output unchanged, so the CLI's single
whole-clip render and the engine's per-block loop agree. Tests:
`crates/asperitas-live/tests/equivalence_resonator.rs` (with a negative control that latches on a
33-frame grid and differs), `equivalence_resonator_automation.rs`, and the counting-allocator check
in `no_alloc.rs`, which covers the resonator's render path.

## Recording

`--record out.wav` writes the frames the engine hands to the device, so a session with knob moves can
be listened to afterwards without a loopback cable. The audio callback only pushes into a lock-free
ring and a writer thread does the file I/O. On exit it prints the frames written and dropped; frames
are dropped (and counted) only if the writer falls two seconds behind, so a nonzero count means the
file has gaps. The file is the engine's output, not what the device played: it cannot show glitches
from the host buffer or the audio path, which you hear on the live output.

## Knob view

On a terminal the knobs are live. For `asperitas`, `q`/`a` raise/lower cutoff and `w`/`s` raise/lower
gain, in steps of 0.05 of knob travel. Holding shift (or typing the uppercase letter) takes a fine step of 0.005.
For `resonator` the keys are `q`/`a` root, `w`/`s` structure, `e`/`d` damping, `r`/`f` spread, with
the same coarse and fine steps, and `t`/`g` the excitation preset, which moves one preset per press
(shift makes no difference, the control has no values between presets). Each line shows the
normalised position and the real-unit value (Hz for root, `n of 3` for the preset). `esc` or
ctrl-c quits. When stdin or stdout is not a terminal the view is disabled: the knobs stay at their
starting values and Enter ends the run.

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
