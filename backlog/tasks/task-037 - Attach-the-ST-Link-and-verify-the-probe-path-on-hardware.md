---
id: TASK-037
title: Attach the ST-Link and verify the probe path on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:28'
updated_date: '2026-09-09 01:31'
labels: []
dependencies:
  - TASK-036
documentation:
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 47500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036 can be written and compile-verified without hardware; none of it is true until a real probe talks to a real board. This ticket also settles the one physical question that could change how the bench is wired: if the SWD pads are unreachable with the Seed in the Pod, then probe-based work and Pod control-surface work may not be simultaneously possible, and that is much cheaper to discover before committing solder.

This is also the only mechanism that recovers a board whose firmware hangs before USB enumerates. The software restart command from TASK-032 needs a live console to receive an instruction, so it cannot help a board that never got that far — the probe can, because it asserts reset from outside.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: whether the SWD pads are reachable with the Seed3 seated in the Pod is determined and recorded, together with the attachment method actually used — soldered hairlines, pogo pins, or running the Seed outside the Pod. Attachment uses the 10-pin Cortex Debug footprint, since the Seed3's extra V3MINIE-style pads are documented as unwired.
- [ ] #2 HUMAN: probe-rs attaches under reset and flashes the application with no interaction with BOOT or RESET.
- [ ] #3 HUMAN: the defmt/RTT log stream is observed live while the application runs, and a forced panic arrives with a decoded backtrace.
- [ ] #4 HUMAN: a deliberately faulted or hung binary is recovered by re-attaching under reset, demonstrating recovery when USB is dead — the case the software restart command cannot serve.
- [ ] #5 HUMAN: probe firmware version is recorded, since probe-rs requires ST-Link V3 firmware 3.2 or newer, along with measured attach time, flash time, log throughput, and anything flaky observed.
<!-- AC:END -->
