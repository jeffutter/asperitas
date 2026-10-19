---
id: TASK-133
title: 'Fix: filter cutoff smoothing calls expf twice per sample on the audio hot path'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 23:08'
updated_date: '2026-10-19 14:14'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 258800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-dsp/src/filter.rs derives alpha from exp(ln_cutoff) and then exp(-2*pi*fc/fs) every sample, even when the smoother has settled. On the Cortex-M7 with software libm this adds two transcendental calls per sample per block. Cache alpha while ln_cutoff is unchanged or compute only while the smoother moves, and name the unexplained 1.001 frozen-cutoff tolerance (alpha jumps from 0 to ~1e-3*2pi/fs there) with a test of that edge. Verify block-time budget impact on host bench or note a HUMAN follow-up for hardware.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-133. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
OnePoleLowPass now caches alpha keyed on the bit pattern of the smoothed ln(cutoff); expf runs only while the smoother moves, and set_sample_rate invalidates the cache. 1.001 tolerance named MIN_CUTOFF_FROZEN_RATIO. Added cache_tests (zero cutoff, frozen-edge bound, cached vs uncached bit-identical, sample-rate change). Workspace tests incl. goldens and gates commit pass. HUMAN follow-up: Cortex-M7 block-time measurement needs the board.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Cached filter alpha so the two expf calls per sample only run while the cutoff smoother moves; named the frozen tolerance and tested the edges; outputs bit-identical.
<!-- SECTION:FINAL_SUMMARY:END -->
