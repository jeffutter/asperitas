---
id: TASK-106.01
title: >-
  HUMAN: re-check LED 1 boot, running and panic states after the shared RgbLed
  refactor
status: Done
assignee:
  - '@human'
created_date: '2026-10-19 03:04'
updated_date: '2026-10-19 16:42'
labels:
  - review-2026-10
  - pod
  - logging
  - hardware
dependencies: []
references:
  - >-
    backlog/tasks/task-120 -
    HUMAN-hardware-recheck-after-the-review-refactors.md
parent_task_id: TASK-106
priority: low
type: chore
ordinal: 260800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
AC1 of TASK-106 (one asperitas_pod::led::RgbLed driver, one polarity constant) shipped in e1fb8ae. The boot LED is on the panic path and was hardware-verified before the refactor, so its states need a board recheck. This can be walked together with TASK-120.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: LED 1 boot-stage red uses the shared RgbLed red channel and polarity (the boot stage lasts a few ms and cannot be seen by eye; verified through the panic state's steady red and the dark off state)
- [x] #2 HUMAN: LED 1 shows the running state while main runs
- [x] #3 HUMAN: LED 1 shows the panic state (solid red) under panictest
- [x] #4 HUMAN: any regression is filed as a bug ticket with the evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Checked with the owner in the TASK-120 bench session (2026-10-19), bootloaded layout, fw 86fe0ae-dirty. AC2: LED 1 steady green while podtest/panictest countdown ran (running state). AC3: LED 1 steady red after the panictest panic, PANIC record also reached USB. AC4: no regression found. AC1 (boot state with the pre-refactor colours and polarity) is NOT ticked: the owner could not clearly see the brief boot red, which is short and sits behind the bootloader's 2 s grace period. Needs a re-look, e.g. blinky (red blink before init, then steady green) or a probe-reset with eyes on LED 1.

AC1 reworded with the owner (2026-10-19): the boot (PreInit) stage is a few ms of steady red before the state flips to Running (see the blinky doc comment), so it cannot be observed by eye; three probe resets of main were watched and showed 2 s dark (bootloader grace) then green with no visible red, as expected. Boot red is set_color_on(true,false,false) on the same shared RgbLed as the panic state, so the red channel and polarity are verified by the panictest steady red, and the dark/off state and green by the running state. Owner agreed to close on that basis.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
LED 1 rechecked on the board with the owner on 2026-10-19 (bootloaded layout, fw 86fe0ae-dirty): steady green in the running state, steady red under panictest's panic with the PANIC record also reaching USB, dark during the bootloader grace period. The boot-stage red is a few ms and unobservable by eye, so it is covered through the shared RgbLed's red channel and polarity proven by the panic state. No regression found. Closed with the owner, who walked through each criterion and agreed it was met.
<!-- SECTION:FINAL_SUMMARY:END -->
