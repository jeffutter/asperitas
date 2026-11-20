---
id: TASK-155.13
title: >-
  Firmware main can run the resonator on the Pod with the chosen control
  mapping, within the callback budget
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
updated_date: '2026-11-20T00:27:00Z'
labels:
  - m5
  - resonator
  - firmware
dependencies:
  - TASK-155.05
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/daisy-seed3.md
  - docs/reference/asperitas-rig.md
  - firmware/src/bin/main.rs
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 308700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Pod firmware `main` currently runs the low-pass and gain patch with the two knobs. The resonator is the effect the project exists for, so `main` must be able to run it with the Pod mapping the owner chose in TASK-155.01 (knobs, encoder, encoder click, buttons, RGB LEDs) and the shared resonator controls (TASK-155.05), without a second definition of the mapping. How the effect is chosen on the device (a build feature, a button, or a boot-time choice) follows that decision record, and the existing patch must remain available.

The audio callback has a hard deadline: a 32-sample block at 48 kHz is about 667 microseconds and the callback may not allocate or wait. The on-device measurement of callback cost against that deadline needs the board and is part of TASK-155.14; this ticket's part is that the code is built to fit: the resonator's host-side cost at the chosen voice count was recorded in TASK-155.04, and nothing here adds work to the callback beyond control latching once per block. Firmware has no host test harness: correctness rests on the cross-build, lints, and the rig, so the control mapping itself is tested as a pure function in the host-testable crates (`asperitas-pod` ControlDecoder or the dsp controls).

Out of scope: running it on the board, with ears or with a timing measurement (TASK-155.14).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Firmware main builds with the resonator selectable per the TASK-155.01 decision, and with the existing patch still available and its behaviour unchanged
- [ ] #2 The Pod inputs map to the resonator's controls through the shared controls definition, not a copy of the mapping; the mapping is covered by a host test of the pure translation, including encoder and button events, with edge cases (out-of-range, NaN) covered
- [ ] #3 The RGB LEDs show what the decision record specifies
- [ ] #4 The callback path allocates nothing and holds no lock across the block, and does only per-block control latching beyond the resonator's own processing
- [ ] #5 cd firmware && make check, clippy and build for the main binary pass with the resonator feature or selection enabled
- [ ] #6 docs/reference/daisy-pod.md or the resonator docs page documents the Pod mapping as built
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
