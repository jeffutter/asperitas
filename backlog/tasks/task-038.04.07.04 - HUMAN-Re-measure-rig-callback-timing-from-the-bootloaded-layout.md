---
id: TASK-038.04.07.04
title: 'HUMAN: Re-measure rig callback timing from the bootloaded layout'
status: Done
assignee:
  - '@human'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-15 16:59'
labels:
  - task
dependencies:
  - TASK-038.04.07.02
parent_task_id: TASK-038.04.07
priority: medium
ordinal: 143800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Code placement changes under the bootloaded layout, so TASK-038.05 bench numbers (max_block_us, worst_gap_us, 72 us callback) predate it. Flash rig bootloaded and re-run the timing measurement; record the new numbers next to the old ones in the rig docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: rig flashed via the bootloader and runs the measurement rig
- [x] #2 HUMAN: new max_block_us / worst_gap_us recorded beside the TASK-038.05 numbers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-15: closed in a live session with the owner at the bench; the owner agreed both criteria were met. rig (LAYOUT=bootloaded) flashed to QSPI 0x90040000 with dfu-util -w after a RESET tap (no probe, so the DWT counter is valid). First boot hard-faulted (HFSR FORCED, CFSR 0x400, stacked PC in Reset): memory-bootloaded.x inserted .sram1_bss (> RAM_D2) AFTER .bss, which pushed __ebss to 0x30000400 and the startup zeroing loop ran off the end of DTCM. Fixed in e5801c7 (INSERT BEFORE .bss plus an ASSERT on __ebss); defect originated in TASK-038.04.07.02. Result, 300 s run: max_block_us 47 (internal 72), worst_gap_us 692 (internal 714), delivered 879 / expected 879, overrun 0, audio_exit 0, dump 60.2 s (internal 69.4 s), 879 blocks and 28,803,072 PCM bytes proved by dump_reassemble, 0 bad frames. dropped_full 36, all at t=1.2-27 s before the host attached (same attach artifact as TASK-073), none during the dump. Recorded in the README bench table (642fac5); capture archived as audio/captures/2026-10-15-rig-sine-300s-bootloaded.console.zst. One transient dfu-util ERASE_PAGE failure on the first flash attempt; an immediate retry against the still-open bootloader succeeded.
<!-- SECTION:NOTES:END -->
