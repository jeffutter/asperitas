---
id: TASK-018.03
title: Decode the Pod encoder and buttons with debounce
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-08 01:14'
labels:
  - planned
dependencies:
  - TASK-018.01
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make the Pod rotary encoder, its click switch, and both pushbuttons usable as decoded events.

Pins, from the table in docs/reference/daisy-pod.md: encoder A = D26 = PD11, encoder B = D25 = PA0, encoder click = D13 = PB6, Button 1 = D27 = PG9, Button 2 = D28 = PA2.

These are SAI2 pins in the Seed pinout, which is harmless here: Seed3 audio runs on SAI1 (docs/reference/daisy-seed3.md). Noting it so the overlap is not mistaken for a conflict mid-implementation.

The `exti` feature is already enabled in firmware/Cargo.toml, so `ExtiInput` is available. Polling is also legitimate, and may be simpler given TASK-018.02 already polls the knobs — one control-surface task servicing everything is easier to reason about than a mix of interrupt and polled paths. Pick one, apply it to all five inputs, and record why. Mixing the two approaches across inputs is the outcome to avoid.

Deliver decoded results, not raw pin state:
- Relative encoder movement as signed detent increments, clockwise positive.
- Press and release edges for the click and both buttons.
- Debounce, so one physical actuation yields exactly one event.

Debounce is not optional polish. The Pod uses mechanical switches and a mechanical encoder; contact bounce on an undebounced quadrature decoder reads as a jumpy, unpredictable parameter — precisely the class of symptom that gets misattributed to DSP, which doc-001 section 7 already flags for gain staging.

Gestures are out of scope. Long-press, double-click, and press-and-hold combinations are deferred: doc-001 sections 7 and 8 defer control-surface decisions until the effect reveals which controls it actually wants. Report edges and let callers compose gestures later.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Encoder rotation yields signed detent increments, with clockwise positive
- [ ] #2 The encoder click and both buttons report press and release edges distinctly
- [ ] #3 One physical press or one detent produces exactly one event — contact bounce does not double-count
- [ ] #4 The debounce interval and the interrupt-versus-poll choice are recorded with the reasoning behind each
- [ ] #5 All five digital inputs use the same approach — the implementation does not mix interrupt-driven and polled inputs
- [ ] #6 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->
