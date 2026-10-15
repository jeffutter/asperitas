---
id: TASK-075
title: Gain maps gain_db through natural exp instead of 10^(dB/20)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 20:33'
updated_date: '2026-10-15 21:26'
labels:
  - planned
dependencies: []
priority: medium
ordinal: 168800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-dsp/src/gain.rs set_params uses libm::expf(gain_db/20), so gain_db 12 yields x1.82 (5.2 dB), not x3.98. Found in TASK-035.04. Fix to 10^(dB/20) and check dependent tests/docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 set_params converts dB to linear as 10^(dB/20), with a unit test at +6.02 dB giving ~x2
- [x] #2 cargo test, clippy and fmt pass; asperitas-measure param_step tests re-baselined if needed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 334cd74. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
gain.rs now uses powf(10, db/20); added test at +6.0206 dB -> x2. param_step tests re-baselined to a true 10.4 dB step (same measured numbers as before: snapped 10.4, smoothed 2.8); thresholds doc comment updated. Full workspace tests, clippy, fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Gain maps gain_db via 10^(dB/20); param_step tests re-baselined to gain_db 10.4 so thresholds are unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
