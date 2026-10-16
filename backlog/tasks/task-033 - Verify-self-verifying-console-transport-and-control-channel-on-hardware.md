---
id: TASK-033
title: Verify self-verifying console transport and control channel on hardware
status: Done
assignee:
  - '@human'
created_date: '2026-09-09 01:25'
updated_date: '2026-10-09 20:24'
labels: []
dependencies:
  - TASK-030
  - TASK-031
  - TASK-032
  - TASK-033.01
documentation:
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-030, TASK-031 and TASK-032 are all verifiable by an agent only as far as code and host-side tests. Whether real bytes survive a real USB link under real load, and whether a real board obeys a real command, can only be settled on hardware — and until they are, every ticket downstream rests on an unproven transport.

This needs one person at the bench once. Flashing still requires holding BOOT and tapping RESET by hand until the debug probe path lands (TASK-036, TASK-037), so this cannot be fully remote. After it passes, captures stop needing hands for anything that does not involve turning a knob, which is what unblocks the poll-rate and detent-ratio tickets.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: the encoder click and both buttons are pressed several times each during a TASK-031 runner capture of at least 240 s, and every press and release appears in the decoded output - this reproduces the specific loss that ate two button presses in TASK-018.04's capture
- [x] #2 HUMAN: if any criterion fails, a bug ticket is filed with the offending timestamps instead of checking the box
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-09: Block lifted. A plain probe flash is recoverable with the debugger, provided the project's probe tooling is used (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach, which can leave the debug port dead until the USB-C is replugged). Boot-path changes (bootloader install, bootloaded layout) stay Blocked.

2026-10-09: Split. The runner capture, integrity check, restart over the control channel and corruption-rate numbers moved to TASK-033.01 (@agent). What stays here is pressing the controls during a capture.

2026-10-16: Closed with the owner. Bootloaded `podtest` (fw 0.1.0+02d99f9) flashed over DFU, 270 s capture through the rig (`captures/task-033-controls-270s.txt`, `--max-drops 100000`). The owner pressed the encoder click, button 1 and button 2 ten times each, about 1 s apart: the decoded output has 10 press and 10 release records for each control, none missing or extra. #1 agreed live. #2: nothing in the criterion failed; the rig verdict was `fail` (discarded_bytes=44, one gap of 296 records at t=13.5-16.4 s, device_dropped=1576), all before the first press at ~36 s, and matches the pre-attach loss TASK-073 explains (the device ran unread after the flash). No bug ticket filed for that reason, agreed with the owner.
<!-- SECTION:NOTES:END -->
