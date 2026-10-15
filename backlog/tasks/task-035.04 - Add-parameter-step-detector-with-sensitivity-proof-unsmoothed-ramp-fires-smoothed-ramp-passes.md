---
id: TASK-035.04
title: >-
  Add parameter-step detector with sensitivity proof (unsmoothed ramp fires,
  smoothed ramp passes)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 19:57'
labels:
  - task
dependencies:
  - TASK-035.01
  - TASK-035.03
parent_task_id: TASK-035
priority: high
ordinal: 165800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In asperitas-measure: detector for audible steps from parameter changes. Test on synthetic signals produced through asperitas-dsp Gain: an intentionally unsmoothed parameter step must be flagged, the smoothed one (existing smooth.rs) must pass, proving the detector can fire. Metric and threshold live in the thresholds module. Covers TASK-035 AC3. Depends on skeleton and metrics.
<!-- SECTION:DESCRIPTION:END -->
