---
id: TASK-152.02
title: >-
  Test: the offline automation render matches the asperitas-live engine bit for
  bit at any host buffer size
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-13T22:00:00Z'
updated_date: '2026-11-19T02:19:00Z'
labels:
  - automation
  - live
  - test
  - planned
dependencies:
  - TASK-152.01
documentation:
  - docs/reference/asperitas-live.md
  - crates/asperitas-live/tests/equivalence_cli.rs
  - crates/asperitas-live/tests/equivalence_device.rs
parent_task_id: TASK-152
priority: medium
type: task
ordinal: 289700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The offline render with an automation file (TASK-152.01) is only a trustworthy stand-in for the device and for `asperitas-live` if moving knobs land on the same samples in all three. The M4 equivalence tests pin fixed Controls (CLI path) and a hand-written knob schedule (device path), but nothing ties the new automation file format to the live engine. This test closes that: the same automation file driven through the live engine core must give the same output as the offline render. It must not open an audio device.

Depends on TASK-152.01, which provides the automation file and the offline render; the live engine needs a way to take the same schedule, and the test should show that applying the schedule through the engine's control slot at the right stream positions reproduces it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An integration test in crates/asperitas-live/tests/ drives the engine core with an automation file (at least one hold, one ramp and one step, on both knobs, with breakpoints at frames that are and are not multiples of 32) and asserts output bit-identical (f32::to_bits) to the offline render of the same clip and file
- [x] #2 The same case runs at host buffer sizes 1, 31, 32, 33, 64, 480, 512, 1024 and a seeded random-length sequence, with identical output across all of them
- [x] #3 A negative control shows the test can fail: applying a breakpoint at the exact frame instead of the next 32-frame boundary makes the comparison fail
- [x] #4 The test opens no audio device, runs within the cargo test gate's time budget, and states its tolerance and what is excluded (the device-boundary clamp) in its module doc
- [x] #5 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 2ba64b7. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-live/tests/equivalence_automation.rs, no production change. Driver publishes controls_at(p) at every 32-multiple and at off-grid breakpoint frames; the boundary publish overwrites the off-grid one before the engine latches. Hosts 1,31,32,33,64,480,512,1024 plus two seeded random sequences are bit-identical to Automation::render. Negative control applies breakpoints at the exact frame and differs. No docs enumerate the equivalence tests, so none updated. gates.sh push passed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New integration test equivalence_automation.rs proves the engine core, fed an automation script through the ControlSlot, is bit-identical to the offline automation render at all host buffer sizes, with a negative control.
<!-- SECTION:FINAL_SUMMARY:END -->
