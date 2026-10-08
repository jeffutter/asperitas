---
id: TASK-038.07.02
title: >-
  HUMAN: Run rig in ring-fill mode on the bench and record claimed versus
  wall-clock capture duration
status: Blocked
assignee:
  - '@human'
created_date: '2026-10-08 13:56'
updated_date: '2026-10-08 15:55'
labels:
  - planned
dependencies:
  - TASK-038.07.01
parent_task_id: TASK-038.07
priority: medium
ordinal: 128800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bench half of TASK-038.07, discharging TASK-038.05 AC #3. Needs TASK-038.07.01's ring-fill build.

Flash with the probe, then boot with the RESET button, because a probe flash zeroes the DWT and CAPSTAT's timing then reads 0. Hold the console open with one long-lived reader for the whole run (see TASK-038.05 notes: a restarted reader lost 19 blocks to the macOS tty buffer). Time the capture with an outside clock from arm to close. Expect about 349.5 s for 1024 blocks of 341.33 ms each, then about 81 s of dump.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: a ring-fill run completes with delivered == 1024, the device's measured capture duration, CAPMAX's seconds_max and an outside wall-clock reading all quoted side by side, with the differences recorded
- [ ] #2 HUMAN: what the producer did when blocks ran out is recorded from the device's own counters (overrun count, audio_exit, whether any block was overwritten), and dump_reassemble proves all 1024 blocks
- [ ] #3 HUMAN: the console capture is archived under audio/captures/ and linked from TASK-038.05's AC #3 note
<!-- AC:END -->
