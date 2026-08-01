---
id: TASK-004.02
title: Flash blinky to the Seed3 and confirm first light
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:56'
labels: []
dependencies:
  - TASK-004.01
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-004
type: feature
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the physical board. This is the moment that proves the board, the USB-C DFU path, and the cross-compiled binary all work — before audio is involved.

Hold `BOOT`, tap `RESET`, release `BOOT`; the board should enumerate as an STM32 DFU device. Then flash and observe.

**If this fails, the board itself is suspect.** Blinky in C++ via libDaisy is a valid isolation step here — libDaisy has no Seed3 *codec* support, but blinky touches no codec. This is the only place in the project where the C++ fallback is available.

Correct docs/reference/daisy-seed3.md if the real procedure differs from what is written there.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: an LED on the Seed3 blinks under firmware built from this repo
- [ ] #2 HUMAN: the documented command sequence works from a clean checkout
- [ ] #3 docs/reference/daisy-seed3.md flashing section reflects the actually-working procedure
<!-- AC:END -->
