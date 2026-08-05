---
id: TASK-018.04
title: Verify every Pod control on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-05 17:27'
labels: []
dependencies:
  - TASK-018.01
  - TASK-018.02
  - TASK-018.03
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board, the Pod, and hands. Nothing in TASK-018.01 through TASK-018.03 is confirmed by a build succeeding — a pin map can compile perfectly and address the wrong pins, and an encoder can count backwards without a single warning.

Needs a firmware binary that surfaces control state. USB CDC serial logging already works (TASK-006), so a `podtest` bin in the mould of firmware/src/bin/ledtest.rs — streaming knob values and control events over serial — is the natural harness. Building that harness is part of this ticket.

Two observations matter more than the rest, because they are the ones that cost time downstream if wrong:

- KNOB JITTER MAGNITUDE. A knob held still should not wander. Record the actual peak-to-peak wobble, in ADC counts or normalised units, so TASK-019 knows whether the smoothing from TASK-018.02 is good enough before a knob drives an audio parameter. A number here is worth far more than "looked stable".

- ENCODER DIRECTION AND DETENT RATIO. Confirm clockwise is positive and that one physical detent produces exactly one increment, in both directions. Both errors are invisible to code review and obvious in the hand.

Also confirm the LED boundary held: LED 1 must still show its boot and panic stages, unchanged. If asperitas-pod disturbed asperitas-logging LED ownership, this is where it shows up.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: both knobs sweep smoothly across the full 0.0 to 1.0 range, reaching the endpoints at their physical stops
- [ ] #2 HUMAN: a knob held still holds its value; the observed peak-to-peak jitter is recorded as a number in the implementation notes
- [ ] #3 HUMAN: the encoder produces one increment per physical detent in both directions, clockwise positive
- [ ] #4 HUMAN: the encoder click and both buttons each register exactly one event per press, with no double-fires
- [ ] #5 HUMAN: LED 2 shows the expected colour for each channel combination, confirming the active-low drive
- [ ] #6 HUMAN: LED 1 still shows its boot and panic stages — asperitas-pod has not disturbed asperitas-logging LED ownership
- [ ] #7 Any correction to the pin map, the LED polarity, or the drive approach is written back into docs/reference/daisy-pod.md
<!-- AC:END -->
