---
id: TASK-138
title: >-
  Hardware recheck: rig images after the rig.rs module split report the same
  RIGCFG/STATUS/CAPSTAT
status: To Do
assignee:
  - '@human'
created_date: '2026-10-19 16:54'
updated_date: '2026-10-19 18:20'
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
- [ ] #2 HUMAN: A stim-excerpt rig image installs and replays a slot, reporting EXCPLAY with clean=1.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-19, live session with the owner at the bench; the owner agreed AC #1 is met. Bootloaded rig built at 8095c78 and flashed with dfu-util after a probe reset opened the bootloader DFU window. RIGCFG, RIGGEN and CAPMAX are byte-identical to the pre-split capture; CAPSTAT and STATUS carry the same fields with dropped_full=0, device_dropped=0, max_block_us 41 against 43 and worst_gap_us 666 against 670. asperitas-rig query and restart now work on a rig image (fw=0.1.0+8095c78 reset_reason=software). AC #2 is not done: the install is refused while a capture is armed or dumping, and the rig dump hangs on hardware since 33f0847, filed as TASK-141. The stim-excerpt image does replay the existing slot 0 clean (EXCPLAY crc16=15fd want=15fd underruns=0 clean=1).
<!-- SECTION:NOTES:END -->
