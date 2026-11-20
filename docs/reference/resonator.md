# Resonator voice

`ResonatorVoice` in `crates/asperitas-dsp/src/resonator.rs` is one tuned voice: it rings at a set
pitch when excited and decays over a set time. A resonator bank is built from many of these. It is
a `Processor` (`no_std`, no allocation, no `Result`) exported from `asperitas-dsp` together with
`ResonatorVoiceParams` and `RESONATOR_VOICE_PARAMS`.

Out of scope: more than one voice, excitation shaping, stereo, and any host wiring (CLI, live, rig,
firmware).

## Parameters

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

## Formulation

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

### Bound

The rotation preserves the state norm, so each step contracts the state by `r < 1` and adds at most
`(1 - r) * |in|`. The state, and so the output, never exceeds the input peak, at any pitch up to
Nyquist and while `r` and `w` change every sample. A long decay therefore has a quiet impulse
response: an impulse of 1 peaks at about `1 - r`, roughly `1.4e-4` at 1 s and `1.4e-5` at 10 s. A
bank applies its own makeup gain.

## Smoothing

`ln(pitch)` and `ln(decay)` are smoothed with a 10 ms time constant (`SMOOTH_MS`); `r`, `sin w` and
`cos w` are derived from the smoothed values every sample (the convention in the `smooth` module).
The coefficients are cached on the bit pattern of the smoothed pair, so a settled voice costs no
`exp`, `sin` or `cos`. Recursive state and input are flushed to exact zero below `1e-30`, so tails
end in silence.

## I/O

- `tick(Frame)` averages the two input channels to one mono input and writes the same output to both.
- `tick_mono(f32) -> f32` is the same voice without the stereo duplication; a bank of voices uses it.
- `ResonatorVoice::new(fs, &params)` has the params in force from the first sample.
  `snap_params()` jumps the smoothers to their targets and keeps the ring; `reset()` also clears it.

## Tests

Unit tests in `resonator.rs` measure the ring pitch (within 0.5 % from 20 Hz to 8 kHz) and the
decay (within 0.5 dB at -30 dB and -60 dB), silence after the tail, reset and block/tick
equivalence, hostile parameters, and a pitch step with a negative control (the same test with
`snap_params()` called after the step must fail). The `processor_laws!` instantiation
`resonator_laws` in `crates/asperitas-dsp/tests/property_tests.rs` runs the shared laws.
