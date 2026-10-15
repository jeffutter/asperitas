---
id: TASK-035.04
title: >-
  Add parameter-step detector with sensitivity proof (unsmoothed ramp fires,
  smoothed ramp passes)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:33'
labels:
  - planned
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Detector in asperitas-measure (new module param_step.rs, registered in lib.rs) reports parameter-step events from a sample slice, with its metric and limit defined only in thresholds.rs with an Origin line
- [x] #2 Test: sine through asperitas-dsp Gain with an unsmoothed gain change (set_params then reset to snap) is flagged at the step position
- [x] #3 Test: same sine and same gain change through the smoothed Gain path is not flagged
- [x] #4 Test: sensitivity margin asserted so the limit sits between the two measured values, and a no-change sine reports zero events
- [x] #5 cargo test -p asperitas-measure, clippy and fmt pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-035.04 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
New param_step.rs (block-RMS detector, lag 1 and 2 comparisons so mid-block steps are not diluted), PARAM_STEP_* constants in thresholds.rs with Origin lines. Discovered Gain.set_params uses natural exp not 10^(dB/20), so tests use gain_db +-24 (10.4 dB real change); follow-up ticket created, not blocking. Margin is 1.5x either side (snapped 10.42 dB, smoothed 2.82 dB, limit 5).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-measure param_step detector with thresholds and sensitivity-proof tests (snapped step flagged in both directions, smoothed passes, steady sine zero events). Tests, clippy, fmt pass.
<!-- SECTION:FINAL_SUMMARY:END -->
