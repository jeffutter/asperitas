---
id: TASK-077
title: 'DSP: make processors survive NaN/inf and flush denormals'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-16 19:47'
labels:
  - review-2026-10
  - dsp
  - planned
dependencies: []
priority: high
type: bug
ordinal: 179800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A NaN gain_db or one NaN input sample poisons Gain and OnePoleLowPass state until reset() (gain.rs:55-58, filter.rs:71-73; f32::clamp passes NaN). Verified by experiment: output stays NaN after 1 s of good input. property_tests.rs only feeds [-1,1] at default params, so nothing catches it. Separately the low-pass tail sits on a subnormal forever (2.68e-43 after 20M samples); no FZ/FTZ is set anywhere, and x86 subnormal maths is 10-100x slower, which matters for the M4 TUI and a feedback resonator.

Change: sanitise parameters in every set_params with NaN-absorbing max/min; sanitise input once at the host boundary (decode_block, read_wav); zero any non-finite or subnormal state in processors with state or feedback; set FTZ/DAZ on the TUI audio thread later (M4) and measure FPSCR.FZ on the device before enabling it.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Gain and OnePoleLowPass recover to finite output within one block after a NaN/inf parameter or input, covered by proptests using any::<f32>() for params and input
- [x] #2 State reaches exactly 0 after enough silence (property test), so goldens do not depend on host FTZ flags
- [x] #3 main's audio path behaviour is unchanged for finite inputs (existing goldens pass untouched)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-077. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Smoother ignores non-finite targets and flushes its value; shared sanitize() (non-finite or |x|<1e-30 -> 0) applied to Gain/filter input and filter state. Gain non-finite gain_db -> 0 dB; filter cutoff uses NaN-absorbing max/min. Host boundary sanitising skipped: read_wav and firmware decode_block convert from integers, so they cannot yield NaN/inf. FTZ/DAZ deferred to M4. Goldens untouched.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
NaN/inf in params or input no longer poisons Gain or OnePoleLowPass, and filter/smoother tails flush to exactly 0; proptests with any::<f32>() added; existing goldens pass unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
