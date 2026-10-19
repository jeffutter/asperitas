---
id: TASK-138
title: >-
  Hardware recheck: rig images after the rig.rs module split report the same
  RIGCFG/STATUS/CAPSTAT
status: Done
assignee:
  - '@human'
created_date: '2026-10-19 16:54'
updated_date: '2026-10-19 19:34'
labels:
  - firmware
  - hardware
dependencies:
  - TASK-086
  - TASK-141
priority: medium
ordinal: 264800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-086 split firmware/src/bin/rig.rs into modules under firmware/src/bin/rig/ as a pure move. Host-side evidence: clippy clean on every rig variant, every variant builds and grades, and image sizes moved by under 0.5 percent from inlining. Only a board can show the behaviour is unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: Flash the bootloaded rig image and capture a session; RIGCFG, STATUS and CAPSTAT match a capture from before the split (same fields, same values for the same build).
- [x] #2 HUMAN: A stim-excerpt rig image installs and replays a slot, reporting EXCPLAY with clean=1.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-19, live session with the owner at the bench; the owner agreed AC #1 is met. Bootloaded rig built at 8095c78 and flashed with dfu-util after a probe reset opened the bootloader DFU window. RIGCFG, RIGGEN and CAPMAX are byte-identical to the pre-split capture; CAPSTAT and STATUS carry the same fields with dropped_full=0, device_dropped=0, max_block_us 41 against 43 and worst_gap_us 666 against 670. asperitas-rig query and restart now work on a rig image (fw=0.1.0+8095c78 reset_reason=software). AC #2 is not done: the install is refused while a capture is armed or dumping, and the rig dump hangs on hardware since 33f0847, filed as TASK-141. The stim-excerpt image does replay the existing slot 0 clean (EXCPLAY crc16=15fd want=15fd underruns=0 clean=1).

2026-10-19, live session with the owner; the owner agreed AC #2 is met and signed off the close. After the TASK-141 fix (a723f4c) a bootloaded rig with ASP_RIG_CAPTURE_SECONDS=10 completed its dump (DUMPEND blocks=30 elapsed_ms=2083 dropped_full=0). With that image running, excerpt_stream installed audio/instruments/octave_mandolin_single_note_soft.wav into slot 0: EXCOK name=octmand_soft bytes=288000 crc16=3add, equal to the host CRC-16/CCITT-FALSE of the WAV data chunk. A stim-excerpt rig image then replayed it: EXCPLAY slot=0 bytes=288000 crc16=3add want=3add underruns=0 clean=1 (slot 0 previously held mand_soft with crc16=15fd). Slot 0 now holds octmand_soft.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both criteria were checked at the bench with the owner. AC1: RIGCFG, RIGGEN and CAPMAX are byte-identical to the pre-split capture and CAPSTAT/STATUS match in fields and values. AC2: an install and replay on a stim-excerpt rig image reports EXCPLAY clean=1. The check also found the rig dump hang from the base64 swap (TASK-141), fixed and confirmed on the board.
<!-- SECTION:FINAL_SUMMARY:END -->
