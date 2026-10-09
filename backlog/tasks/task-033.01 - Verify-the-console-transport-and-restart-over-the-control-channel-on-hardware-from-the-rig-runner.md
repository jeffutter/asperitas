---
id: TASK-033.01
title: >-
  Verify the console transport and restart over the control channel on hardware,
  from the rig runner
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 20:24'
labels: []
dependencies:
  - TASK-030
  - TASK-031
  - TASK-032
parent_task_id: TASK-033
priority: high
type: task
ordinal: 151800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent half of TASK-033, split out 2026-10-09 at the owner's request. Everything here is a capture through the TASK-031 runner and a host command over the TASK-032 control channel, which needs no hands once the board is flashed over the probe path (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach). The remaining human part of TASK-033 is pressing the encoder click and both buttons during a capture. Record numbers, not pass/fail.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With the board flashed to the build containing TASK-030 and TASK-032, a capture of at least 240 s taken through the TASK-031 runner reports zero frames failing the integrity check, against the 8.8% line-corruption baseline recorded in TASK-018.04
- [ ] #2 The host requests a restart and the board comes back and re-enumerates with nobody touching BOOT or RESET, and the reset reason reported afterwards identifies a software-initiated restart
- [ ] #3 The before and after corruption rates are recorded as numbers in the implementation notes
- [ ] #4 If any criterion fails, a bug ticket is filed with the offending timestamps from the capture artifact instead of checking the criterion off
<!-- AC:END -->
