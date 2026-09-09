---
id: TASK-027.01
title: Flash podtest and leave the bench rigged for scripted capture
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:29'
labels: []
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
- [ ] #1 HUMAN: podtest is flashed onto the Seed3 over DFU, entering the bootloader by holding BOOT and tapping RESET, and confirmed running.
- [ ] #2 HUMAN: the board is left cabled to the host with its console reachable, and the serial device path is recorded in this ticket's implementation notes so later captures can name it.
- [ ] #3 HUMAN: knobs, encoder and buttons are left at a documented resting position, since hold-window measurements depend on nothing being touched.
<!-- AC:END -->
