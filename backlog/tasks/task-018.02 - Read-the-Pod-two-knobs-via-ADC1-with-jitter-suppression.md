---
id: TASK-018.02
title: Read the Pod two knobs via ADC1 with jitter suppression
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-05 17:26'
labels:
  - planned
dependencies:
  - TASK-018.01
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make both Pod potentiometers readable as normalised values, ready for TASK-019 to map onto DSP parameters.

Knob 1 is D21 = PC4; Knob 2 is D15 = PC0. Both are ADC1-capable on the STM32H750 (PC4 = ADC12_INP4, PC0 = ADC123_INP10), so a single `Adc<ADC1>` covers both — verify the channel impls compile rather than assuming. `p.ADC1` survives `new_daisy_board!` because the macro only partially moves `p`.

Pot jitter is called out in doc-001 section 7 as a Medium risk specifically because it presents as a DSP bug. embassy-stm32 0.6 ADC v4 has hardware averaging built in — `AdcConfig::averaging` accepts up to `Averaging::Samples1024`. Prefer that to a hand-rolled filter: it costs nothing at runtime and needs no state. Layer additional smoothing on top only if measurement shows it is needed, and say what the measurement was.

Report position as a normalised f32 in the inclusive range 0.0 to 1.0.

Curve shaping is explicitly NOT this ticket. A log or exponential taper for frequency-like parameters is parameter-mapping policy, and doc-001 section 3 puts that with the caller: the DSP owns parameter semantics, the host owns the knob-to-parameter mapping, and the BSP owns neither. It reports where the knob is. The taper belongs in the shared mapping that TASK-019.01 builds, so that the CLI, the Pod, and the M4 TUI all get the same curve instead of three subtly different ones.

Sample at a rate the control surface needs — roughly 1 kHz is ample for a hand-turned knob — and keep it off the audio callback path entirely. The callback runs every 32 samples (0.67 ms) and has no budget for ADC conversions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both knobs read as a normalised f32 in the inclusive range 0.0 to 1.0, monotonically increasing with physical rotation in one direction
- [ ] #2 Knob sampling happens outside the audio callback
- [ ] #3 ADC hardware averaging is configured, and the chosen sample count is recorded with the reasoning behind it
- [ ] #4 The jitter-suppression approach is stated in the implementation notes; measuring the residual on real hardware is TASK-018.04
- [ ] #5 A degenerate or out-of-range raw ADC reading cannot produce a value outside 0.0 to 1.0, and cannot panic
- [ ] #6 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->
