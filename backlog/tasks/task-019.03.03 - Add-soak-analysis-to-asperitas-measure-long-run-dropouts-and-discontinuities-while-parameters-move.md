---
id: TASK-019.03.03
title: >-
  Add soak analysis to asperitas-measure: long-run dropouts and discontinuities
  while parameters move
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 01:40'
labels:
  - task
  - planned
dependencies:
  - TASK-019.03.01
parent_task_id: TASK-019.03
priority: high
ordinal: 175800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
AC #2 and #3: a 5+ minute capture with parameters moving must show no silence dropouts and no discontinuity candidates above threshold, and the step detector must fire on an unsmoothed build. The pieces exist (metrics.rs silence/discontinuity candidates, param_step.rs), but nothing runs them as one verdict over a long swept capture or correlates candidates with parameter-change times. Add an analyze mode that takes a capture and its override schedule, runs silence, discontinuity and param-step detection over the whole run, reports candidates inside versus outside parameter-motion windows, and returns a pass/fail verdict with an explicit expect-failure option so the unsmoothed capture can be asserted to fail as evidence of sensitivity. Test on synthetic 5 minute-scale data (smoothed ramp passes, unsmoothed ramp fails, injected dropout fails).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Soak mode returns one verdict over silence, discontinuity and param-step results with candidates attributed to parameter-motion windows
- [x] #2 Expect-failure option lets an unsmoothed capture be asserted to fail
- [x] #3 Synthetic tests: smoothed passes; unsmoothed ramp and injected dropout each fail
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED in the TASK-019.03.03 commit (see git log --grep TASK-019.03.03). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
New crates/asperitas-measure/src/soak.rs (judge_soak, SoakReport with inside/outside/unattributed attribution), CLI --soak/--expect-fail in cli.rs, SOAK_* constants in thresholds.rs, docs in asperitas-rig.md. Deviation from plan: the fixed-gain discontinuity model false-fires on a swept gain, so metrics::discontinuities gained GainFit::Tracking (per-10 ms least-squares gain refit) and the shared DelayedModel; the existing fixed-gain behaviour is unchanged (metrics tests pass). expected_failure_observed requires step count > 0 and no silence, since a dropout's edges also read as level steps. Synthetic 310 s tests: smoothed 30 dB ramp passes; snapped 6 dB plateaus fail with all steps inside the window while the same schedule smoothed gives zero steps; injected dropouts fail with silence attributed outside/inside; short capture fails. Hardware captures stay with TASK-019.03.04.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added soak analysis to asperitas-measure: analyze --soak FROM:TO [--expect-fail] returns one pass/fail verdict over silence, discontinuity and parameter-step detection with candidates attributed to the motion window, tracking-gain discontinuity detection for swept parameters, SOAK_* thresholds, docs, and synthetic 5-minute tests plus CLI tests.
<!-- SECTION:FINAL_SUMMARY:END -->
