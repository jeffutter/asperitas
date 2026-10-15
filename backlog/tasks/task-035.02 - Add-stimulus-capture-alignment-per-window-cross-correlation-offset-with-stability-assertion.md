---
id: TASK-035.02
title: >-
  Add stimulus/capture alignment: per-window cross-correlation offset with
  stability assertion
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:18'
labels:
  - task
  - planned
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Add pub mod align to asperitas-measure: per-window offset (integer samples plus parabolic subsample) of capture vs regenerated stimulus via FFT cross-correlation, no new thresholds as literals
- [x] #2 Stability assertion: offsets constant across windows (tolerance in thresholds.rs) and equal between the 2026-10-13 reset1/reset2 pair; slip returns a typed error, not a tolerated drift
- [x] #3 Tests: 300 s sine and 10 s repeat pair fixtures, plus synthetic delayed signal (recovers delay) and synthetic one-sample slip mid-signal (detected)
- [x] #4 cargo test --workspace and clippy pass with no board attached
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by f43943d. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added align.rs (rustfft, pure Rust). Measured fixtures: delay 33.057 +-0.002 samples across all windows of the 300 s sine and both 10 s reset captures; thresholds set from that (tol 0.25 vs 1.0 for a one-sample slip). A pure sine makes delay ambiguous modulo 48 samples, so the first window searches the full range and takes the smallest delay among tied peaks (anchor), later windows track within +-8 samples of the anchor. A slip of a whole period is undetectable, documented in the module.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-measure::align: per-window FFT cross-correlation delay with parabolic subsample, assert_stable and assert_repeatable returning typed AlignError (Slip, RepeatMismatch). Limits live in thresholds.rs. Tests on 300 s sine, reset1/reset2 pair, synthetic delay and one-sample slip. Workspace tests and clippy pass.
<!-- SECTION:FINAL_SUMMARY:END -->
