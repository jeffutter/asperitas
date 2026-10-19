---
id: TASK-145.07
title: >-
  HUMAN: listen to asperitas-live with the corpus clips and an instrument and
  confirm it sounds like the device
status: To Do
assignee:
  - '@human'
created_date: '2026-10-19 22:10'
updated_date: '2026-10-19 22:10'
labels:
  - m4
  - live
  - hardware
dependencies:
  - TASK-145.01
  - TASK-145.02
  - TASK-145.03
  - TASK-145.04
  - TASK-145.05
  - TASK-145.06
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/daisy-pod.md
parent_task_id: TASK-145
priority: medium
type: task
ordinal: 280700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The tests prove the live host computes what the device computes; they cannot prove it is usable for sound design or that nothing between the engine and the speakers (device buffer, clamp, channel routing, glitches under load, key response) spoils it. Only listening does. Compiling, and the bit-identity test, are not evidence for this.

Run asperitas-live (the run command is in the doc TASK-145.03 added) on the Mac:
- with each clip in audio/instruments as the looping source, turning both knobs from the keyboard across their whole range;
- with an instrument on line-in (through an interface or preamp at line level; the Pod gain-staging note in docs/reference/daisy-pod.md applies to the comparison);
- side by side with the Pod running firmware `main` at roughly matching knob positions, on the same clip or instrument.

File any defect found as its own bug ticket under this umbrella with what was heard and the command used.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: every corpus clip loops with no glitch other than the clip's own end-to-start jump, with the effect audible and both knobs responding while it plays
- [ ] #2 HUMAN: knob moves from the keyboard are smooth, no zipper noise or clicks, and the fine step is usable
- [ ] #3 HUMAN: an instrument on line-in plays through with latency the owner finds acceptable for playing, and no dropouts over a few minutes
- [ ] #4 HUMAN: at matching knob positions the live host and the Pod running main sound the same on the same source
- [ ] #5 HUMAN: any defect heard is filed as a bug ticket under TASK-145
<!-- AC:END -->
