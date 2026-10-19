---
id: TASK-126
title: 'Fix: asperitas CLI process clamps out-of-range params unlike gain/filter'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 01:16'
updated_date: '2026-10-19 03:36'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 244800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-cli/src/process.rs: asperitas processor maps real values via ParamSpec::to_normalised which clamps, so gain_db=20 gives +12 dB and cutoff_hz=0 does not mute, while gain/filter do not clamp. Reject out-of-range for asperitas or document+test. Also export a single ASPERITAS_PARAMS table from asperitas.rs instead of the CLI chaining FILTER_PARAMS and GAIN_PARAMS with only a debug_assert.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas process rejects out-of-range or non-finite param values with an error naming the key and range
- [x] #2 A single ASPERITAS_PARAMS table is exported from asperitas.rs and used by the CLI without a debug_assert
- [x] #3 Tests cover rejection and boundary acceptance; cargo test, clippy and fmt pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 447eba3. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ASPERITAS_PARAMS (asperitas.rs, re-exported); CLI asperitas arm uses it and rejects non-finite/out-of-range values naming key and range. Tests: rejection and boundary acceptance. test, clippy, fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas process now rejects out-of-range/non-finite params; single ASPERITAS_PARAMS table replaces the chained tables and debug_assert.
<!-- SECTION:FINAL_SUMMARY:END -->
