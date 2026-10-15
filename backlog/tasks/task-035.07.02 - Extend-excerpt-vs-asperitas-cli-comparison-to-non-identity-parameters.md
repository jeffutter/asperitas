---
id: TASK-035.07.02
title: Extend excerpt-vs-asperitas-cli comparison to non-identity parameters
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 20:52'
updated_date: '2026-10-15 21:15'
labels:
  - task
  - planned
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
- [x] #1 Comparison accepts processor and parameters and runs asperitas-cli with the same settings
- [x] #2 Tolerances for non-identity settings documented; synthetic-capture tests pass and a mismatched-parameter capture fails
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this ticket's commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reference::from_excerpt_wav_fx takes FxSetting (CTL FX code + wire value) and runs asperitas_dsp ReplayFx, same knob = v/65535 as firmware. analyze gained --fx/--fx-value; report carries fx. Reference is processed, so loop-gain deviation stays vs LOOP_GAIN_DB; same limits apply at all settings (documented in thresholds.rs with synthetic numbers; real-device noise caveat deferred to 035.07.03). Tests: five matching settings pass, five mismatches fail (gain deviation, alignment slip), bypass regression, unknown code typed error.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Excerpt comparison now accepts a processor selection (bypass/gain/lowpass + wire value) and runs the same ReplayFx for the reference; documented tolerances and docs updated; synthetic-capture tests pass and mismatched settings fail.
<!-- SECTION:FINAL_SUMMARY:END -->
