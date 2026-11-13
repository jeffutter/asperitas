---
id: TASK-152.05
title: >-
  HUMAN: run an automated knob session on the board and compare it with the
  offline render
status: To Do
assignee:
  - '@human'
created_date: '2026-11-13T22:01:00Z'
labels:
  - automation
  - rig
  - hardware
dependencies:
  - TASK-152.04
  - TASK-152.02
documentation:
  - docs/reference/asperitas-rig.md
  - docs/reference/flashing.md
  - audio/README.md
parent_task_id: TASK-152
priority: medium
type: task
ordinal: 292700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The device half of this umbrella: the offline render with an automation file (TASK-152.01) is the stand-in the agents will rely on, and the rig session (TASK-152.04) lets the board run the same file. This ticket shows the board, with the knobs moving, agrees with the offline render on real audio, the way TASK-145.06 did for held knobs. It is @human because it needs the Pod rigged with its self-loopback cable and the board flashed.

Board set-up is the one used in TASK-145.06: the rig `stim-excerpt` image from HEAD, bootloaded layout, an installed corpus excerpt (check the console's EXCPLAY crc16 against the clip the reference uses; a slot can hold a different clip than assumed), the ST-Link attached for `--probe-reset`, and `--device` set to the Seed3's console node when two usbmodem nodes are present. An agent may prepare the commands, build and flash, and analyse the captures with the owner live; the close is the owner's.

Suggested automation: one file that moves both knobs, with a ramp on knob 0 (cutoff, log taper), a step on knob 1 (gain) and a hold, long enough to cover the excerpt. Commit captures, sidecars and reports under captures/TASK-152.05/.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: the rig image built from HEAD after TASK-152.04 is flashed with an excerpt in the slot the reference uses, and the automation file used is committed beside the captures
- [ ] #2 HUMAN: an automated session with moving knobs passes asperitas-measure analyze --excerpt against the offline render of the same clip and file, within the comparison's documented limits
- [ ] #3 HUMAN: a negative control fails as it should (the same capture analysed against a different automation file or against bypass)
- [ ] #4 HUMAN: captures, sidecars and reports are committed under captures/TASK-152.05/ and the ticket notes record the commands used
<!-- AC:END -->
