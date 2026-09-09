---
id: TASK-038.01
title: Add stimulus synthesis processors to asperitas-dsp with host property tests
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:31'
labels: []
dependencies: []
modified_files:
  - crates/asperitas-dsp/src/stimulus.rs
  - crates/asperitas-dsp/src/lib.rs
  - crates/asperitas-dsp/tests/property_tests.rs
parent_task_id: TASK-038
priority: high
type: task
ordinal: 55500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-035 deconvolves a measured return against a stimulus it must be able to reproduce exactly, and TASK-019.03 sweeps parameters against it. Neither is possible unless the waveform comes from one implementation that firmware and host can both reason about. This ticket owns that implementation and nothing else: no firmware, no transport, no SDRAM.

Add `crates/asperitas-dsp/src/stimulus.rs` with three sources behind the existing `Processor` trait (`tick` primitive, typed `Params`, `reset`, no allocation, no `Result`, libm-only math so the module stays `no_std`):

- **Sine** — a wrapping integer phase accumulator (NCO), not per-sample `sinf` and not a 2-pole recurrence. The accumulator wraps exactly, so frequency is exact, phase starts on a sample boundary, and amplitude cannot drift across a multi-minute run. Frequencies are chosen as integer periods per window (coherent sampling) so host analysis needs no leakage window.
- **Exponential sweep (ESS)** — exponential chirp, not linear. Equal energy per octave keeps the recovered impulse response usable and places harmonic distortion outside it. No amplitude pre-equalization: pre-emphasis time-smears the recovered IR (Farina's refinements paper).
- **Impulse train** — a peak-normalized band-limited pulse (windowed sinc / Dirichlet kernel), not a one-sample delta. A naive click is flat to Nyquist by construction and puts its energy exactly where the analog loop is least trustworthy.

One level convention for all three: `level_dbfs` f32, default −20 dBFS, the level TASK-034 records loop gain at.

This module is also the single owner of the machine-readable description of what was played, because device and host must never disagree about it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `crates/asperitas-dsp/src/stimulus.rs` provides three stimulus sources (sine, exponential sweep, band-limited impulse train) that implement `Processor` with typed `Params`, no allocation, no `Result`, libm-only float math, and they compile into the firmware dependency graph without `std`.
- [ ] #2 Sine uses an integer phase accumulator whose wrap is exact: a host test asserts that a window whose length is an integer number of periods contains an integer number of cycles, and that relative frequency error stays below 1e-7.
- [ ] #3 Amplitude is exact rather than approximate: peak magnitude equals the requested `level_dbfs` within a stated tolerance, is identical across repeated runs, and is unchanged after 17,600,000 samples (ten minutes) so long captures cannot silently sag.
- [ ] #4 The sweep spans its documented f0..f1 exponentially over an exact integer sample count and reports its endpoints; the impulse train emits a peak-normalized pulse whose energy is spread over more than one sample, asserted against a naive one-sample click.
- [ ] #5 Each source has a `describe()` returning its parameter set as a space-separated `key=value` string (name, sample rate, level, frequency or chirp endpoints, repetition period), and that method is the only place the grammar is written; a host test pins the exact strings so renaming a parameter fails CI instead of quietly desynchronising device from host.
- [ ] #6 `crates/asperitas-dsp/tests/property_tests.rs` covers all three sources for finite output, bounded output, reset idempotence, block-equals-tick, and bit-identical determinism, and `cargo test --workspace` passes with default (no_std) features.
- [ ] #7 Where waveform shapes overlap `crates/asperitas-cli/src/synth.rs`, the two implementations are compared in a host test or their difference is documented with a reason, so neither diverges silently.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Reference points in this repo

- `crates/asperitas-dsp/src/processor.rs` — `Frame = [f32; 2]`, `Processor` with associated `Params: Clone + Default`; `process_block` is provided and zips to the shorter side, never panics.
- `crates/asperitas-dsp/src/gain.rs` / `filter.rs` — house style: scalar f32 per-sample `tick`, `libm::*` only, output `.clamp(-1.0, 1.0)`, params mapped through a `params_from_normalised(knob)` helper, std-gated CLI parsing kept out of the firmware path.
- `crates/asperitas-dsp/src/smooth.rs` — `pub(crate) Smoother`, usable here for click-free ramps at module start/stop.
- `crates/asperitas-cli/src/synth.rs` — existing host-side `generate_sweep` accumulates phase in **f64** via libm, and its doc comment is the recorded reason: f32 ULP at ~18,000 rad exceeds the 1e-4 golden tolerance and platform libm variance already broke goldens once. Accumulate sweep/NCO phase in f64 for the same reason; the M7 FPU makes the cost irrelevant at 48 kHz.
- `crates/asperitas-dsp/tests/property_tests.rs` — proptest 1, strategies build blocks of 4..64 / 64..256 / 256..512 frames; properties are named `output_always_finite`, `output_bounded`, `silence_in_silence_out`, `reset_idempotent`, `block_equals_tick`, `param_change_smooth`. Match those names for the new sources.
- Firmware consumes this crate without `std` (`firmware/Cargo.toml`), so anything using `f32::sin` instead of `libm::sin` breaks the cross build, which CI runs as `cd firmware && cargo build --release --features seed3`.

## Prior art worth citing in the code comments

- Renesas TB318, "The NCO as a Stable, Accurate Synthesizer"; U. Maine NCO tutorial; ADI "An Almost Pure DDS Sine Wave Tone Generator".
- Farina, "Simultaneous measurement of impulse response and distortion with a swept-sine technique" (AES 5083) plus its refinements paper — clock mismatch smearing (eliminated here by one codec), synchronous averaging cancelling late-tail HF, amplitude pre-equalization time-smearing the IR.
- Puckette et al., "Alias-Free Digital Synthesis of Classic Analog Waveforms"; Acoustics Engineering TN008 (Dirac stimuli note) for MLS vs linear vs exponential tradeoffs.
- Metric vocabulary TASK-035 will need and should not have to invent: tone, THD, THD+N, SNR, SINAD, SFDR, dynamic range, stepped-frequency sweep, crosstalk.

## Decisions already made here, do not re-open

- Exponential, not linear, sweep. Band-limited pulse, not a delta. Integer-period frequencies. Phase accumulation in f64. Level in dBFS with −20 dBFS default.
- Deliberate dithering, if any, belongs to a later ticket that has characterized the noise floor; uncharacterized truncation spurs read as distortion, so leaving dither out until measured is the honest default.
<!-- SECTION:NOTES:END -->
