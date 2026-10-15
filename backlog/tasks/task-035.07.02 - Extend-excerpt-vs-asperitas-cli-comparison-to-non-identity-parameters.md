---
id: TASK-035.07.02
title: Extend excerpt-vs-asperitas-cli comparison to non-identity parameters
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 20:52'
labels:
  - task
dependencies: []
parent_task_id: TASK-035.07
priority: medium
ordinal: 171800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the TASK-035.05 comparison so it can compare a device capture against asperitas-cli output at non-identity processor settings (Gain, filter), with documented tolerance per setting. Verify using synthetic captures generated host-side; real device capture is a separate HUMAN subtask.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Comparison accepts processor and parameters and runs asperitas-cli with the same settings
- [ ] #2 Tolerances for non-identity settings documented; synthetic-capture tests pass and a mismatched-parameter capture fails
<!-- AC:END -->
