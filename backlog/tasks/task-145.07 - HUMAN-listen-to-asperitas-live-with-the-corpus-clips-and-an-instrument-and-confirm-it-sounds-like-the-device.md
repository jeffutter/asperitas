---
id: TASK-145.07
title: >-
  HUMAN: listen to asperitas-live with the corpus clips and an instrument and
  confirm it sounds like the device
status: To Do
assignee:
  - '@human'
created_date: 2026-10-19 22:10
updated_date: '2026-11-13T19:26:00Z'
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
- [x] #1 HUMAN: every corpus clip loops with no glitch other than the clip's own end-to-start jump, with the effect audible and both knobs responding while it plays
- [x] #2 HUMAN: knob moves from the keyboard are smooth, no zipper noise or clicks, and the fine step is usable
- [ ] #3 HUMAN: an instrument on line-in plays through with latency the owner finds acceptable for playing, and no dropouts over a few minutes
- [ ] #4 HUMAN: at matching knob positions the live host and the Pod running main sound the same on the same source
- [x] #5 HUMAN: any defect heard is filed as a bug ticket under TASK-145
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-13: Partly done live with the owner; #3 and #4 are open and the ticket stays To Do.

Heard from a `--record` capture (fb193bb) of asperitas-live played back on the Mac, not from the live output alone: the owner listened to the recording, believes all of it, and heard no loop glitch and a working effect with both knobs responding (#1 ticked on that statement; the owner could not say they had listened to every corpus clip). Knob moves from the keyboard were smooth; no zipper noise or clicks reported (#2). Holding a key does not repeat well, so the owner taps the key; the owner judges this a terminal key-repeat limit, not a defect, so no bug ticket was filed (#5, no defect heard).

Not done: #3 (instrument on line-in, latency, a few minutes of dropouts): the clips are pre-recorded, so latency could not be judged, and the Mac has only the built-in mic as an input (a TRRS-split cable to the headphone jack is not offered as an input). The owner will come back to it later. #4 (live host vs the Pod running main by ear): not checked; the owner said to assume it works, which is not evidence, so it stays unticked. The Pod currently runs the rig image from TASK-145.06, so main needs reflashing before any comparison. The bit-identity of the live engine to asperitas-cli is covered by TASK-145.05 and a 4 s --record capture matched asperitas-cli exactly on the first 144000 frames.
<!-- SECTION:NOTES:END -->
