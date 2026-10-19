---
id: TASK-099.05
title: >-
  HUMAN: bench run reproduces a TASK-019.03.04 style result with the session
  command
status: To Do
assignee:
  - '@human'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-19 19:48'
labels:
  - task
dependencies:
  - TASK-099.04
  - TASK-142
parent_task_id: TASK-099
priority: medium
ordinal: 249800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
HUMAN: with the board and probe attached, run asperitas-rig session then asperitas-measure analyze, and confirm the result matches the TASK-019.03.04 evidence. Commit artifacts with this ticket per docs/reference/asperitas-rig.md.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-19 bench attempt with the owner (loopback cable fitted, rig built from a723f4c with ASP_RIG_CAPTURE_SECONDS=60): asperitas-rig session --probe-reset refused to send FX because the BOOT record never reaches the host on a bootloaded rig (stream starts at seq 5). Nothing was swept and there is nothing to analyse. Filed as TASK-142; rerun this ticket once it lands.
<!-- SECTION:NOTES:END -->
