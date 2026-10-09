---
id: TASK-027.01
title: Flash podtest and leave the bench rigged for scripted capture
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 01:29'
updated_date: '2026-10-09 11:08'
labels:
  - planned
dependencies: []
parent_task_id: TASK-027
priority: high
type: task
ordinal: 111
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-027 exists because TASK-025 AC #3 was closed without its hardware measurement. The measurement itself needs no hands once a capture can be scripted — but getting the board flashed and rigged does, until the debug probe path removes the need for a hand on BOOT and RESET.

Splitting it this way isolates the one genuinely physical act from the analysis that only looked physical because nobody had a runner yet. Leaving the bench rigged rather than packing it away afterwards is what lets TASK-027.02 and TASK-029 run unattended.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 podtest is built and flashed onto the Seed3 with the project's probe path (make probe-flash, chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach), and confirmed running from its console output
- [x] #2 The board's serial device path is recorded in this ticket's implementation notes so later captures can name it
- [x] #3 The knobs, encoder and buttons are assumed untouched since the owner last left them; a measurement that depends on a resting position says so rather than claiming it
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying this ticket update. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-09: Reassigned @human -> @agent. The probe path removes the BOOT+RESET handshake (docs/reference/daisy-seed3.md), so flashing and recording the serial path need no hands. Only the resting position of the controls was physical, and it is now an assumption stated in the measurement rather than an act.

2026-10-09: Block lifted. A plain probe flash is recoverable with the debugger, provided the project's probe tooling is used (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach, which can leave the debug port dead until the USB-C is replugged). Boot-path changes (bootloader install, bootloaded layout) stay Blocked.

Flashed podtest with 'make probe-flash BINARY=podtest' from firmware/ (the Makefile variable is BINARY, not BIN; chip ASPERITAS_H750IB via asperitas-h750.yaml, under-reset default, STLink V3 0483:3754:002700283235511838363730, verify OK). Confirmed running from the framed console: BOOT proto=1 fw=0.1.0, then '[podtest] running' and periodic r1/r2 ADC lines. Serial device path: /dev/cu.usbmodem1101 (macOS, USB product 'Asperitas Debug Console'; /dev/cu.usbmodem102 is the ST-Link VCP, not the console). Knobs, encoder and buttons are ASSUMED untouched since the owner last left them; any measurement depending on a resting position must say so. Idle knob reads were r1~65531 r2~65531-65533. Boot path untouched.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
podtest built and flashed via probe path, confirmed running from console output, console device recorded as /dev/cu.usbmodem1101, bench left rigged. No source changes.
<!-- SECTION:FINAL_SUMMARY:END -->
