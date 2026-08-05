---
id: TASK-019
title: >-
  Wire asperitas-dsp into the firmware audio callback with knobs driving
  parameters
status: To Do
assignee:
  - '@human'
created_date: '2026-08-05 17:27'
labels:
  - planned
dependencies:
  - TASK-018
documentation:
  - docs/reference/daisy-pod.md
priority: high
type: feature
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Second half of milestone M3 in doc-001, and the milestone payoff: "the first time the device makes a sound you can change with your hands."

firmware/src/bin/main.rs today runs a bare passthrough — `output.copy_from_slice(input)`. asperitas-dsp is already a declared firmware dependency and entirely unused. This ticket closes that gap and puts the Pod knobs from TASK-018 in charge of the parameters.

This is plumbing, not sound design. The resonator bank is M5. What is being proven here is that the whole chain — knob to ADC to shared parameter mapping to Processor to codec to your ears — works end to end, so that M5 has somewhere to land.

Umbrella ticket. It is `@human` because it inherits the strictest assignee among its children: TASK-019.02 needs ears.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks are Done
<!-- AC:END -->
