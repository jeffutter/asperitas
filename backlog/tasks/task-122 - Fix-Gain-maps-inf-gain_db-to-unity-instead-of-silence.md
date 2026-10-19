---
id: TASK-122
title: 'Fix: Gain maps -inf gain_db to unity instead of silence'
status: Done
assignee:
  - '@agent'
created_date: '2026-10-16 20:18'
updated_date: '2026-10-19 03:24'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 227800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-dsp/src/gain.rs set_params, any non-finite gain_db falls back to 0 dB. f32::NEG_INFINITY is mathematically zero gain, so a mute or fade to -inf passes the signal at full level. Map -inf to linear 0 (silence); keep the unity fallback only for NaN and +inf (or clamp +inf to the ceiling). Add a unit test for -inf. Also consider making OnePoleLowPass NaN cutoff policy consistent with Gain and documenting it, and make OnePoleLowPass::default() derive its smoother from cutoff_hz rather than starting at 1.0.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-122. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Gain::set_params now maps -inf to silence, +inf to the 8192 ceiling, NaN to unity. Added non_finite_tests. OnePoleLowPass default already derived its smoother from cutoff; NaN policy left as closed filter and documented in filter.rs and gain.rs. Tests run with --all-features (plain cargo test -p asperitas-dsp fails on missing std in asperitas.rs, pre-existing). gates commit passed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed Gain -inf gain_db to produce silence; +inf saturates at ceiling, NaN stays unity; unit tests added; NaN policy divergence documented.
<!-- SECTION:FINAL_SUMMARY:END -->
