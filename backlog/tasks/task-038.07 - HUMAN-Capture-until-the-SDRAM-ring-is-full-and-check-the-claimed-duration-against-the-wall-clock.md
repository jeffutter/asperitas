---
id: TASK-038.07
title: >-
  HUMAN: Capture until the SDRAM ring is full and check the claimed duration
  against the wall clock
status: To Do
assignee:
  - '@human'
created_date: '2026-10-08 13:55'
labels:
  - planned
dependencies: []
parent_task_id: TASK-038
priority: medium
ordinal: 126800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Split out of TASK-038.05 AC #3 on 2026-10-08 at the owner's request. TASK-038.05 asks for a capture that runs until the ring reports full, comparing the seconds the device claimed with the wall-clock run and recording what the producer did when blocks ran out. rig cannot do that as built: `CAPTURE_SECONDS` is const-gated strictly below the ring (`expected_blocks(window) < capture::RING_BLOCKS`, rig.rs), so `ASP_RIG_CAPTURE_SECONDS=400` fails the build. A ring-filling run therefore needs a firmware mode first (TASK-038.07.01, @agent) and then a bench session (TASK-038.07.02, @human). Umbrella: its only criterion is that both subtasks are Done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Subtasks TASK-038.07.01 and TASK-038.07.02 are Done
<!-- AC:END -->
