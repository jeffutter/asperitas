---
id: TASK-019.03.02
title: >-
  Add sweep-response analysis to asperitas-measure: monotonic direction and
  flat-region check per parameter
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 01:27'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-019.03
priority: high
ordinal: 174800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
AC #1 needs an objective answer to: as a parameter is swept across its full travel under host control, does the measured response (RMS level for gain, band-limited level or spectral centroid for low-pass) move monotonically in the intended direction with no flat regions. Add this to asperitas-measure: given an aligned capture plus the sweep schedule (the SET times from asperitas-rig's override run), bucket the response against parameter value, and judge direction, monotonicity (with a documented noise tolerance from thresholds.rs, the single place for thresholds) and flat regions. Expose as an analyze mode/flag and JSON report. Prove sensitivity with synthetic fixtures: a monotonic response passes; a response with a plateau, a reversal, and a dead zone each fail. No hardware needed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Analysis reports direction, monotonic verdict and flat regions for gain and low-pass sweeps, with thresholds defined only in thresholds.rs
- [x] #2 Synthetic tests: monotonic passes; plateau, reversal and dead-zone fixtures each fail
- [x] #3 Usage documented alongside the other analyze modes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-019.03.02. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-measure/src/sweep.rs (judge_sweep), an analyze --sweep FROM:TO --sweep-kind gain|lowpass mode in cli.rs, SWEEP_* thresholds in thresholds.rs only, synthetic tests (monotonic, descending, plateau, reversal, dead zone, wrong direction, ripple, short) and a CLI usage test, and docs in docs/reference/asperitas-rig.md. Schedule is recomputed from linear time (control.rs schedule is linear) rather than shared.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-measure analyze --sweep judges response direction, monotonicity and flat regions per gain/lowpass sweep, with thresholds only in thresholds.rs and synthetic fixtures proving each failure mode. Real-capture thresholds are revisited in TASK-019.03.04.
<!-- SECTION:FINAL_SUMMARY:END -->
