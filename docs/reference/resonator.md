# Resonator

The resonator work is split so each part can be judged and measured without the others. This page
records the parts that exist.

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
