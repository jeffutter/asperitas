---
id: TASK-035.03
title: >-
  Add objective capture metrics: level, silence, discontinuity candidates, spur
  report, loop-gain normalization
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 19:57'
labels:
  - task
dependencies:
  - TASK-035.01
  - TASK-035.02
parent_task_id: TASK-035
priority: high
ordinal: 164800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In asperitas-measure: peak/RMS, runs of silence, sample-discontinuity candidates (count and positions), spectral spur report, all machine-readable (serde JSON) and normalized against the TASK-034 loop gain (-0.41 dB at -20 dBFS/1 kHz). Document what deviations can and cannot be attributed to (output+input converters combined; common-mode faults like shared clock or channel swap are NOT covered, see TASK-034 criterion 2). Fixture tests assert on the metrics against thresholds. Covers TASK-035 AC2. Depends on skeleton and alignment.
<!-- SECTION:DESCRIPTION:END -->
