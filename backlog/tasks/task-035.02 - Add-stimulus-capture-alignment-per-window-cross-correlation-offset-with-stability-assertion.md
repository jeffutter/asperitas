---
id: TASK-035.02
title: >-
  Add stimulus/capture alignment: per-window cross-correlation offset with
  stability assertion
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 19:57'
labels:
  - task
dependencies:
  - TASK-035.01
parent_task_id: TASK-035
priority: high
ordinal: 163800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In asperitas-measure: FFT or block cross-correlation of stimulus vs capture, reporting integer(+subsample) offset per window. Assert the offset is constant across windows and across repeated captures (single clock domain, no slip) rather than tolerating drift. Tests on fixtures (2026-10-13-rig-10s-reset1/reset2 repeat pair, 300 s sine, sweep) plus synthetic delayed/slipped signals proving a slip is detected. Covers TASK-035 AC1. Depends on the crate skeleton.
<!-- SECTION:DESCRIPTION:END -->
