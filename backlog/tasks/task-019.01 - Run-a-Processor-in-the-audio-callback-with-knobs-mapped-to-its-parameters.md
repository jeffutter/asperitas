---
id: TASK-019.01
title: Run a Processor in the audio callback with knobs mapped to its parameters
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-05 17:27'
updated_date: '2026-08-07 23:53'
labels:
  - planned
dependencies:
  - TASK-018.02
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-019
priority: high
type: feature
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the passthrough in firmware/src/bin/main.rs with an `asperitas_dsp::Processor` driven by the Pod knobs.

Use the processors that already exist rather than writing new DSP. `OnePoleLowPass` on knob 1 is the right first choice because a filter sweep is unmistakable by ear — when the goal is proving a whole signal chain, an ambiguous result is worthless. `Gain` on knob 2 gives a second, independently audible axis. The resonator is M5; do not start it here.

THE KNOB-TO-PARAMETER MAPPING MUST BE SHARED, NOT FIRMWARE-LOCAL. TASK-008.01 established the pattern: `parse_params_from_cli` lives on each processor type inside asperitas-dsp, next to its `Params` definition, so every host imports one mapping instead of inventing its own. A normalised-knob mapping belongs in the same place for the same reason. If the device and the CLI apply different curves, they stop sounding alike, and comparing them — which is how doc-001 section 7 proposes to distinguish gain-staging problems from DSP problems — stops meaning anything. This is also where the knob taper deliberately excluded from TASK-018.02 belongs.

Real-time constraints, per doc-001 section 3: no allocation and nothing fallible in the callback. Update parameters at block rate, not per sample. Smoothing already lives inside the processors (`Smoother` in asperitas-dsp), which is what makes a per-block `set_params` safe from zipper noise — do not add a second smoothing layer in the firmware.

Two facts to build against rather than assume:
- Blocks are 32 samples, not the 48 that doc-001 section 1 assumed. daisy-embassy hardcodes `BLOCK_LENGTH = 32` (src/audio.rs:13). This is known and recorded under TASK-011.
- The sample rate is 48 kHz: `AudioConfig::default()` is `Fs::Fs48000`, confirmed during TASK-005.02. Call `set_sample_rate` explicitly anyway rather than relying on a processor default, which is what doc-001 section 3 designed that method for.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The audio callback runs a Processor from asperitas-dsp instead of copying input to output
- [ ] #2 Both Pod knobs drive processor parameters through a mapping defined in asperitas-dsp and shared with asperitas-cli, not defined in firmware
- [ ] #3 asperitas-cli can be driven with the same normalised knob positions as the device and produces the same parameter values, so device and desktop output are comparable on identical source material
- [ ] #4 set_sample_rate is called with the device actual rate rather than relying on the processor default
- [ ] #5 The callback performs no heap allocation and cannot panic at any knob position, including both extremes
- [ ] #6 Parameters are updated at block rate, and no smoothing is duplicated in the firmware on top of the processor own
- [ ] #7 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->
