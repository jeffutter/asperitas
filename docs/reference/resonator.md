# Resonator

The resonator work is split so each part can be judged and measured without the others. This page
records the parts that exist.

## Resonator voice

`ResonatorVoice` in `crates/asperitas-dsp/src/resonator.rs` is one tuned voice: it rings at a set
pitch when excited and decays over a set time. A resonator bank is built from many of these. It is
a `Processor` (`no_std`, no allocation, no `Result`) exported from `asperitas-dsp` together with
`ResonatorVoiceParams` and `RESONATOR_VOICE_PARAMS`.

Out of scope: more than one voice, stereo, and any host wiring (CLI, live, rig, firmware). Excitation
shaping is the separate stage below.

### Parameters

`RESONATOR_VOICE_PARAMS` is the only source of range, taper and default.

| name       | unit | range      | taper | default | meaning                                      |
|------------|------|------------|-------|---------|----------------------------------------------|
| `pitch_hz` | Hz   | 20 - 8000  | log   | 110     | ring frequency (110 Hz is A2)                |
| `decay_s`  | s    | 0.05 - 10  | log   | 1.0     | time for the ring to fall by 60 dB (T60)     |

The ranges are what a knob sweeps. The processor sanitises values outside them:

- NaN pitch or decay becomes the default.
- Pitch is floored at 1 Hz and held at 0.49 times the sample rate, so a pitch at or above Nyquist
  rings just below it. The ceiling is re-applied when the sample rate changes, so
  `set_params` and `set_sample_rate` commute.
- Decay is clamped to 1 ms - 60 s, so zero or negative decay is the shortest decay, not a freeze.

### Formulation

A damped rotation: the Gordon-Smith "magic circle" coupled oscillator with a pole radius. State
`(x, y)` is updated each sample:

```
x' = r * (cos w * x - sin w * y) + (1 - r) * in
y' = r * (sin w * x + cos w * y)
out = y'
w = 2 * pi * pitch / fs
r = exp(-ln(1000) / (decay * fs))
```

Considered and not used: a direct-form or RBJ biquad band-pass (interpolated coefficient pairs can
leave the stable region while pitch glides), a Chamberlin state-variable filter (unstable near
fs/6, no hard output bound at high pitch), and the undamped magic circle on its own (it has no
decay). `asperitas-dsp` depends only on `libm`, so no filter crate was available to reuse.

#### Bound

The rotation preserves the state norm, so each step contracts the state by `r < 1` and adds at most
`(1 - r) * |in|`. The state, and so the output, never exceeds the input peak, at any pitch up to
Nyquist and while `r` and `w` change every sample. A long decay therefore has a quiet impulse
response: an impulse of 1 peaks at about `1 - r`, roughly `1.4e-4` at 1 s and `1.4e-5` at 10 s. A
bank applies its own makeup gain.

### Smoothing

`ln(pitch)` and `ln(decay)` are smoothed with a 10 ms time constant (`SMOOTH_MS`); `r`, `sin w` and
`cos w` are derived from the smoothed values every sample (the convention in the `smooth` module).
The coefficients are cached on the bit pattern of the smoothed pair, so a settled voice costs no
`exp`, `sin` or `cos`. Recursive state and input are flushed to exact zero below `1e-30`, so tails
end in silence.

### I/O

- `tick(Frame)` averages the two input channels to one mono input and writes the same output to both.
- `tick_mono(f32) -> f32` is the same voice without the stereo duplication; a bank of voices uses it.
- `ResonatorVoice::new(fs, &params)` has the params in force from the first sample.
  `snap_params()` jumps the smoothers to their targets and keeps the ring; `reset()` also clears it.

### Tests

Unit tests in `resonator.rs` measure the ring pitch (within 0.5 % from 20 Hz to 8 kHz) and the
decay (within 0.5 dB at -30 dB and -60 dB), silence after the tail, reset and block/tick
equivalence, hostile parameters, and a pitch step with a negative control (the same test with
`snap_params()` called after the step must fail). The `processor_laws!` instantiation
`resonator_laws` in `crates/asperitas-dsp/tests/property_tests.rs` runs the shared laws.

## Excitation stage

Code: `crates/asperitas-dsp/src/excitation.rs`, exported as `Excitation`, `ExcitationParams`,
`EXCITATION_PARAMS` and `EXCITATION_PRESETS`. It is an ordinary stereo `Processor` and needs no
resonator bank to run, test or listen to (`asperitas-dsp` is `no_std` and allocation free).

A resonator bank driven by the raw instrument signal rings with the low body of the signal and its
loudest transients, which is harsh and level dependent. The stage shapes what reaches the bank and
keeps the drive bounded. Each channel is processed independently with identical coefficients, so a
mono signal on either or both sides behaves the same.

### Signal flow

1. Low cut: a one-pole high-pass (input minus a one-pole low-pass of it).
2. Transient emphasis: the signal is multiplied by `1 + 2 * transient * attack`. `attack` is
   `max(fast - slow, 0) / (fast + 1e-4)`, with `fast` and `slow` envelope followers (1 ms and 40 ms)
   of the high-passed magnitude. It is a ratio, so it does not depend on level, and it is 0 in a
   steady tone or a decay. The multiplier is at most 3.
3. Soft limiter: the identity up to half the ceiling, then a tanh knee that is continuous in value
   and slope and approaches the ceiling from below. It is last, so the output never exceeds the
   ceiling whatever the other stages do.

### Parameters

| Name | Unit | Range | Taper | Default | Why |
|------|------|-------|-------|---------|-----|
| `low_cut_hz` | Hz | 10 - 1000 | log | 10 | Removes the low body that makes a bank boom. At 10 Hz it is about -0.29 dB at 41 Hz (bass low E), so the default is transparent. Smoothed as `ln(Hz)` over 5 ms. |
| `transient` | - | 0 - 1 | linear | 0 | Attack emphasis. 0 leaves the signal as the high-pass made it. Smoothed over 5 ms, the same speed as the low cut, so a step in both does not combine into a click. |
| `drive_limit` | amplitude | 0.25 - 2.0 | linear | 2.0 | Limiter ceiling. At 2.0 the linear region reaches 1.0, full scale. Smoothed over 10 ms. |

NaN parameters fall to the default; infinities clamp to the range. Input samples that are not
finite are treated as 0.

### Presets

`EXCITATION_PRESETS` is data only; nothing wires it to a control yet.

| Preset | `low_cut_hz` | `transient` | `drive_limit` |
|--------|-------------|-------------|---------------|
| 1 gentle (equals `Default`) | 10 | 0 | 2.0 |
| 2 medium | 80 | 0.3 | 1.0 |
| 3 hard | 200 | 0.6 | 0.5 |

### Measured behaviour

From the tests in `excitation.rs` and `crates/asperitas-dsp/tests/property_tests.rs`:

- Default gain of 0.25 amplitude sines (dB): 41 Hz -0.29, 82 Hz -0.07, 165 Hz -0.02, 330 Hz -0.01,
  660 Hz and above about -0.006. The test requires every one within 0.5 dB of unity.
- Raising `low_cut_hz` from 10 to 200 lowers a 55 Hz sine by over 10 dB and moves 2 kHz by less
  than 0.25 dB (measured -0.15).
- `transient = 1` raises the peak of the first 10 ms of a 220 Hz note by over 3 dB and leaves its
  tail (from 150 ms on) within 1 dB.
- Lowering `drive_limit` lowers the peak of a full-scale sine, which stays under the ceiling.
- Bound: output magnitude is at most `min(drive_limit_max, 6 * input_peak)`. The high-pass output is
  at most twice the input peak, the transient multiplier at most 3, and the limiter never raises a
  magnitude. Property tests cover finite and bounded output, silence in and out, `process_block`
  equal to repeated `tick`, `reset` forgetting history, parameter steps without clicks, and hostile
  parameters and input.
