---
id: TASK-152.02
title: >-
  Test: the offline automation render matches the asperitas-live engine bit for
  bit at any host buffer size
status: To Do
assignee:
  - '@agent'
created_date: '2026-11-13T22:00:00Z'
labels:
  - automation
  - live
  - test
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
- [ ] #1 An integration test in crates/asperitas-live/tests/ drives the engine core with an automation file (at least one hold, one ramp and one step, on both knobs, with breakpoints at frames that are and are not multiples of 32) and asserts output bit-identical (f32::to_bits) to the offline render of the same clip and file
- [ ] #2 The same case runs at host buffer sizes 1, 31, 32, 33, 64, 480, 512, 1024 and a seeded random-length sequence, with identical output across all of them
- [ ] #3 A negative control shows the test can fail: applying a breakpoint at the exact frame instead of the next 32-frame boundary makes the comparison fail
- [ ] #4 The test opens no audio device, runs within the cargo test gate's time budget, and states its tolerance and what is excluded (the device-boundary clamp) in its module doc
- [ ] #5 bash scripts/gates.sh push passes
<!-- AC:END -->
