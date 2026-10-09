---
id: TASK-038.07.02
title: >-
  HUMAN: Run rig in ring-fill mode on the bench and record claimed versus
  wall-clock capture duration
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-08 13:56'
updated_date: '2026-10-09 20:23'
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
- [ ] #1 Before the full run, a short capture under 'probe-rs run' (probe stays attached, using the project's chip description ASPERITAS_H750IB) shows a nonzero max_block_us, proving cycle-counter figures are valid on this path. If they read 0, stop, leave this ticket Blocked with the finding, and do not substitute a probe memory read during a capture
- [ ] #2 A ring-fill run completes with delivered == 1024, the device's measured capture duration, CAPMAX's seconds_max and an outside wall-clock reading (host clock, arm to close) all quoted side by side, with the differences recorded
- [ ] #3 What the producer did when blocks ran out is recorded from the device's own counters (overrun count, audio_exit, whether any block was overwritten), and dump_reassemble proves all 1024 blocks
- [ ] #4 The console capture is archived under audio/captures/ and linked from TASK-038.05's AC #3 note
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-09: Block lifted. A plain probe flash is recoverable with the debugger, provided the project's probe tooling is used (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach, which can leave the debug port dead until the USB-C is replugged). Boot-path changes (bootloader install, bootloaded layout) stay Blocked.

2026-10-09: Reassigned @human -> @agent at the owner's request. No hands are needed: probe flash, one long-lived console reader, a host clock. The RESET-button boot the original text asked for is replaced by a run under 'probe-rs run' (docs/reference/daisy-seed3.md, 'A probe detach stops the cycle counter'), verified by the first criterion. Never read memory through the probe during a capture.
<!-- SECTION:NOTES:END -->
