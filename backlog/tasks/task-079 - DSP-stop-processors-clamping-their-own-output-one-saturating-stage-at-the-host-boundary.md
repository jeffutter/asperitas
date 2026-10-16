---
id: TASK-079
title: >-
  DSP: stop processors clamping their own output; one saturating stage at the
  host boundary
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-16 22:32'
labels:
  - planned
dependencies:
  - TASK-077
priority: medium
type: chore
ordinal: 181800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Gain, OnePoleLowPass and all three stimuli hard-clip to +-1 (gain.rs:64-67, filter.rs:73). That makes them nonlinear (Gain +12 dB on 0.5 gives [1,1]), clips between chain stages, and makes the *_output_bounded proptests (property_tests.rs:47-57, 175-185) pass by construction, so they would never see a runaway resonator, the M5 property that matters. encode_block (main.rs:139-140) and wav_io.rs:70-71 already clamp at the boundary.

Change: remove the per-processor clamps, keep the boundary clamps (and non-finite sanitising), make boundedness tests assert a real bound on unclamped output.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No processor in asperitas-dsp clamps its own output; the codec/WAV boundary still saturates
- [x] #2 Boundedness proptests assert a real bound on unclamped output
- [x] #3 main's audible behaviour is unchanged (Gain is last in its chain and encode_block clamps identically)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 6ebf05f. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Removed the output clamps from Gain, OnePoleLowPass and the sine/sweep/pulse stimuli. Boundary clamps in firmware audio.rs (decode/encode_block), cli wav_io.rs and measure compare.rs remain. Gain proptest now asserts |out| <= |in|*max(1,gain); filter asserts |out| <= peak input; stimulus tests already assert the level ceiling. main chain ends in Gain and encode_block clamps, so audible output is unchanged.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Per-processor +-1 clamps removed from asperitas-dsp; saturation happens only at the codec/WAV boundary. Boundedness proptests now assert real bounds on unclamped output.
<!-- SECTION:FINAL_SUMMARY:END -->
