---
id: TASK-122
title: 'Fix: Gain maps -inf gain_db to unity instead of silence'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 20:18'
labels:
  - review-fix
dependencies: []
ordinal: 227800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-dsp/src/gain.rs set_params, any non-finite gain_db falls back to 0 dB. f32::NEG_INFINITY is mathematically zero gain, so a mute or fade to -inf passes the signal at full level. Map -inf to linear 0 (silence); keep the unity fallback only for NaN and +inf (or clamp +inf to the ceiling). Add a unit test for -inf. Also consider making OnePoleLowPass NaN cutoff policy consistent with Gain and documenting it, and make OnePoleLowPass::default() derive its smoother from cutoff_hz rather than starting at 1.0.
<!-- SECTION:DESCRIPTION:END -->
