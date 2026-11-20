# Resonator

The resonator work is split so each part can be judged and measured without the others. This page
records the parts that exist: the voice, the excitation stage and the bank that composes them.

## Resonator voice

`ResonatorVoice` in `crates/asperitas-dsp/src/resonator.rs` is one tuned voice: it rings at a set
pitch when excited and decays over a set time. A resonator bank is built from many of these. It is
a `Processor` (`no_std`, no allocation, no `Result`) exported from `asperitas-dsp` together with
`ResonatorVoiceParams` and `RESONATOR_VOICE_PARAMS`.

Out of scope here: more than one voice (see the bank below), stereo, and any host wiring (CLI, live, rig, firmware). Excitation
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

## Resonator bank

Code: `crates/asperitas-dsp/src/resonator_bank.rs`, exported as `Resonator`, `ResonatorParams`,
`RESONATOR_PARAMS`, `VOICE_COUNT` and `BANK_GAIN`. It is the effect itself: a `Processor`
(`no_std`, no allocation, no `Result`) beside `Asperitas`, which is unchanged. The tuning model,
voice count and default voicing follow `backlog/decisions/decision-001 - M5-resonator-tuning-voices-and-pod-mapping.md`:
a root note plus a structure series, the root a plain input to the bank so a pitch tracker can
later drive it, 8 voices, and a default voicing of A2, structure 0, damping 0.5, gentle excitation,
wide spread. Not part of it: the knob mapping (`Controls`), host wiring, pitch tracking, modulation.

### Signal flow

1. The two input channels are averaged to one: `0.5 * (l + r)`.
2. `Excitation` shapes it. The stage is stereo, so it is fed the mono signal on both channels and
   channel 0 is taken; the second channel is wasted work, accepted to reuse the stage unchanged.
3. Each of the 8 `ResonatorVoice`s rings from that signal (`tick_mono`).
4. Each voice output is scaled by `BANK_GAIN` (2), a smoothed mute gain and an equal-power pan
   gain, and summed into left and right.

### Parameters

`RESONATOR_PARAMS` is the only source of range, taper and default for the four bank fields. The
excitation fields are the nested `ResonatorParams::excitation` and are described by
`EXCITATION_PARAMS` above. NaN becomes the default and everything else clamps.

| name        | unit | range     | taper  | default | meaning |
|-------------|------|-----------|--------|---------|---------|
| `root_hz`   | Hz   | 20 - 2000 | log    | 110     | pitch of voice 1 (110 Hz is A2); the processor holds 1 Hz - 20 kHz |
| `structure` | -    | 0 - 1     | linear | 0       | 0 pure harmonic series, 1 most stretched (inharmonic) |
| `damping`   | -    | 0 - 1     | linear | 0.5     | higher is a shorter, darker ring |
| `spread`    | -    | 0 - 1     | linear | 1       | 0 mono, 1 widest placement; smoothed over 10 ms |

### Tuning model

Voice `n` (1 to 8) rings at `root * n * sqrt(1 + B * (n^2 - 1))` with `B = structure * 0.02`. This
is the stiff-string stretch of a harmonic series: structure 0 gives `n * root`; at structure 1
partial 8 sits at about 12 times the root instead of 8, and the root itself never moves. It is a
single smooth control that goes from a pure series to a bell-like one. A partial above 0.49 of the
sample rate has its gain glided to zero (10 ms) instead of stacking at the voice's pitch ceiling.
Pitch changes are smoothed by the voices (`ln(pitch)`, 10 ms).

### Damping

Root T60 is `10 s * 0.01^damping`: 10 s at 0, 1 s at 0.5 (the default), 0.1 s at 1. Partial `n`
rings for the root T60 divided by `1 + 0.5 * damping * (n - 1)`, so the more the bank is damped
the sooner its higher partials die: the tail is shorter and darker together. At damping 0 every
partial rings 10 s. `ResonatorParams::root_decay_s` gives the root T60. Decay changes are
smoothed by the voices (`ln(decay)`, 10 ms).

### Spread

Voice `i` (0-based) sits at pan `(-1)^(i+1) * (i + 1) / 8`: alternating left and right, the root
at -0.125 and the highest partial at +1 (hard right). The effective pan is that times `spread`.
Gains are `cos` and `sin` of `pi/4 * (pan + 1)`, evaluated as `(cos t -/+ sin t) / sqrt(2)` with
`t = pi/4 * pan`, so equal power at any pan and, at pan 0, one expression for both channels:
spread 0 gives left equal to right bit for bit. Pan gains are recomputed only when the smoothed
spread changes.

### Makeup gain and bound

A voice is unit gain at its resonance and its impulse response is small (see Bound above), so the
bank applies `BANK_GAIN = 2`, which also returns the 3 dB the centre of the field loses to equal
power. The bound is

```
|out| <= BANK_GAIN * VOICE_COUNT * min(2.0, 6 * input_peak)
```

(a voice never exceeds its input, an equal-power gain never exceeds 1, the excitation output never
exceeds `min(drive_limit_max, 6 * peak)`): 32 for a full-scale input. It holds while parameters
glide. It is a bound, not a level.

### Cost

`cargo run --release -p asperitas-dsp --example bank_cost` times `process_block` over 32 frames.
Host measurement (not the Cortex-M7; the device check is the budget check on the rig, with the
decision record's target of at most 25 % of the 667 us block, about 165 us):

| case | per 32-frame block | share of 667 us |
|------|--------------------|-----------------|
| settled params, default voicing | 2.14 us | 0.3 % |
| params moved every block (every voice recomputes `exp`, `sin`, `cos` every sample) | 8.92 us | 1.3 % |

### Tests

Unit tests in `resonator_bank.rs`: the ring pitch of every partial at structure 0 and 1 (within
0.5 %), the damping direction with a swapped negative control, the default one-second root ring,
spread 0 giving identical channels and spread 1 putting voice 8 only on the right (with the
spread 0 negative control), muting above the pitch ceiling, silence after the tail, `reset`
idempotence and determinism, `process_block` equal to repeated `tick`, a parameter step
under 0.08 per sample against a snapped-smoother negative control, and hostile parameters. The
`processor_laws!` instantiation `resonator_bank_laws` and `bank_worst_case_stays_under_the_bound`
in `crates/asperitas-dsp/tests/property_tests.rs` cover finite and bounded output (longest
decay, both ends of structure, the whole root range, full-scale and non-finite input).
