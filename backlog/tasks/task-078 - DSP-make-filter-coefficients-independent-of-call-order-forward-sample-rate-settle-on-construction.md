---
id: TASK-078
title: >-
  DSP: make filter coefficients independent of call order, forward sample rate,
  settle on construction
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
labels:
  - review-2026-10
  - dsp
dependencies: []
priority: high
type: bug
ordinal: 180800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
OnePoleLowPass::set_params bakes alpha from the current sample_rate_hz and set_sample_rate only updates the smoother (filter.rs:52-67). Verified: 1 kHz at 8 kHz after 4 unit-step samples is 0.408 with set_params then set_sample_rate and 0.957 the other way. ReplayFx never receives a sample rate (fx_chain.rs:96-98 forwards it to the generator only), so its Gain/LowPass stay at 48 kHz. A cpal device at 44.1 kHz would silently detune every filter. Also a fresh processor ramps from its Default smoother state, so CLI output and goldens are wrong for the first ~50 ms (process.rs:27-30, golden_tests.rs:77-80).

Change: each processor stores its sanitised Params and derives all coefficients in one private recompute(), called from set_params and set_sample_rate; forward the rate through ReplayFx and FxChain; offer settled construction (P::new(sample_rate, &params) or reset() after first set_params, enforced).

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A proptest shows set_params/set_sample_rate in either order gives identical output
- [ ] #2 ReplayFx and FxChain forward the sample rate to their effects
- [ ] #3 A processor built with requested params produces settled output from sample 0 (CLI and golden tests updated, filter goldens regenerated with a listen if they move)
<!-- AC:END -->
