---
id: TASK-035.03
title: >-
  Add objective capture metrics: level, silence, discontinuity candidates, spur
  report, loop-gain normalization
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:26'
labels:
  - planned
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 metrics module reports peak, RMS (dBFS and LSB), longest/total silence runs, discontinuity candidate count and positions, and spur report (largest non-fundamental bins with frequency and dBFS), as serde-serializable JSON
- [x] #2 levels are normalized against thresholds::LOOP_GAIN_DB so deviation is reported as dB from the TASK-034 baseline; any new limit lives in thresholds.rs with an origin comment
- [x] #3 fixture tests on the 2026-10-07 300 s and 2026-10-15 bootloaded captures assert metrics inside thresholds, and a synthetic capture with an injected step, dropout and spur proves each detector fires
- [x] #4 module docs state what deviations are attributable (output plus input converters combined) and what is not (shared clock, channel swap; see TASK-034 criterion 2)
- [x] #5 cargo test -p asperitas-measure and cargo clippy pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-035.03 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-measure/src/metrics.rs (analyze -> Metrics, serde JSON) and thresholds with origin comments. Fixtures: gain deviation 0.005/0.003 dB, no silence, max residual jump 1.83 LSB, worst spur -92 dBc. Synthetic step, dropout and 3 kHz spur each detected. Debug-mode tests take ~58 s because of the 300 s fixtures.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented metrics module (level/loop-gain deviation, silence runs, residual-based discontinuity candidates, averaged-FFT spur report) with fixture and injected-fault tests; test and clippy pass.
<!-- SECTION:FINAL_SUMMARY:END -->
