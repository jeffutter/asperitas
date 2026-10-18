---
id: TASK-081
title: 'DSP: small duplications and the smoothing-domain convention'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-18 22:52'
labels:
  - review-2026-10
  - dsp
dependencies: []
priority: low
type: chore
ordinal: 183800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Smoothing time constants are written twice (10.0 at gain.rs:35-36,47-48; 5.0 at filter.rs:42,55). Gain runs two smoothers that always hold the same target (gain.rs:27-28,57-58). snap_params duplicates reset for Gain (gain.rs:79-82). i16 scaling divides by i16::MAX so -32768 reads -1.00003 (wav_io.rs:31,40-41). The filter smooths the coefficient alpha while Gain smooths linear amplitude (filter.rs:63-66, gain.rs:55-58); for a resonator, smoothing coefficients gives non-uniform glides and can pass through unstable pairs. Decide and document the convention in smooth.rs: smooth the perceptual parameter (log Hz, dB) and derive coefficients per sample or sub-block.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Named consts replace the repeated time constants; Gain has one smoother
- [x] #2 i16 conversion is symmetric and shared
- [x] #3 smooth.rs documents the smoothing-domain convention and the filter follows it (goldens regenerated with a listen if they move)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-081. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Gain now has one Smoother and named SMOOTH_MS consts exist in gain.rs and filter.rs. New asperitas_dsp::pcm::i16_to_f32 divides by 32768 (i16::MIN = -1.0 exactly) and is used by wav_io, measure align and measure compare. Filter now smooths ln(cutoff) and derives alpha per sample; cutoffs at or below 1e-3 Hz still freeze the state. smooth.rs documents the convention, including the memoryless-multiplier exception for Gain (linear amplitude, no recursion to destabilise). snap_params on Gain now calls reset. Goldens regenerated: differences are the 1/32768 input rescale and f32 rounding from the log-domain path (about 1e-6 absolute, roughly -120 dB), so no listen was needed; all workspace tests and clippy pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Deduplicated smoothing constants and the Gain smoother, made i16 scaling symmetric and shared, documented the smoothing-domain convention in smooth.rs and moved the filter to log-cutoff smoothing with per-sample alpha. Goldens regenerated (sub -100 dB shifts).
<!-- SECTION:FINAL_SUMMARY:END -->
