---
id: TASK-155.14
title: >-
  HUMAN: run the resonator on the board, compare the rig capture with the
  offline render, and play it on the Pod
status: To Do
assignee:
  - '@human'
created_date: '2026-11-20T00:27:00Z'
labels:
  - m5
  - resonator
  - hardware
dependencies:
  - TASK-155.12
  - TASK-155.13
  - TASK-155.10
documentation:
  - docs/reference/asperitas-rig.md
  - docs/reference/flashing.md
  - docs/reference/daisy-pod.md
  - audio/README.md
  - captures/TASK-152.05
parent_task_id: TASK-155
priority: medium
type: task
ordinal: 309700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Everything about the resonator that can be shown on the host has been (render, goldens, live equivalence, stage equivalence). What remains needs the board and a person: that the device computes what the offline render computes, that the callback fits its deadline, and that it behaves and sounds right on the Pod with a real instrument. Compiling is not evidence.

Two parts, in order. (1) Rig: select the resonator stage over `CTL FX`, replay a corpus excerpt through it at fixed settings and at an automation-driven sweep, capture and compare with the offline render using `asperitas-measure analyze --excerpt`, as was done for the M4.5 knob session (captures/TASK-152.05/); commit the artifacts under captures/TASK-155.14/ per audio/README.md and measure the callback cost against the 32-sample (about 667 microseconds) block deadline at the chosen voice count. (2) Pod: flash firmware `main` with the resonator and play it. Pod audio I/O is line level, not hi-Z; use a DI or preamp (docs/reference/daisy-pod.md).

An agent must not pick this up unattended. An agent in a live session with the owner may tick criteria only after walking through each one with them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: the rig capture at fixed resonator settings agrees with the offline render within the excerpt comparison's documented residual limit, with artifacts committed under captures/TASK-155.14/
- [ ] #2 HUMAN: the rig capture under an automation-driven damping/structure sweep agrees with the offline render within the same limit, and a negative control (different settings) fails the comparison
- [ ] #3 HUMAN: the callback cost at the chosen voice count is measured on the board and recorded, under the block deadline with the margin the TASK-155.01 decision record asks for, or the numbers are reported and a follow-up ticket is filed
- [ ] #4 HUMAN: firmware main with the resonator is flashed and plays an instrument through the Pod with no dropouts, glitches or runaway ringing, and the knobs, encoder, buttons and LEDs behave as the decision record specifies
- [ ] #5 HUMAN: the owner confirms the Pod sounds like the review page and live host, or each difference is filed as a ticket
- [ ] #6 Close note records that it was done live with the owner, so the history shows a person signed off
<!-- AC:END -->
