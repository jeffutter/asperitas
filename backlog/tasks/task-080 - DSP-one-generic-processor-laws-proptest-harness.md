---
id: TASK-080
title: 'DSP: one generic processor-laws proptest harness'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
labels:
  - review-2026-10
  - dsp
  - testing
dependencies:
  - TASK-077
  - TASK-078
priority: medium
type: chore
ordinal: 182800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
property_tests.rs:32-154 and :160-287 are near-identical copies per processor and weaker than they read: laws run at default params only; *_reset_idempotent gives both instances the same history so it never tests 'regardless of prior history' (processor.rs:25-26); param_change_smooth checks only the first sample and warms up 256 samples against a 480-sample time constant; FxChain::reset deliberately breaks the trait's reset contract (fx_chain.rs:113-117) with no test saying so.

Change: one generic processor_laws::<P>(arb_params) harness covering arbitrary params, different histories before reset, max per-sample delta across a whole param step, and NaN/inf inputs, so each new effect gets the suite in one line.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A generic harness runs the same laws for Gain and OnePoleLowPass with arbitrary params and differing prior histories
- [ ] #2 The copied per-processor property tests are deleted
- [ ] #3 The FxChain::reset contract deviation is documented and tested
<!-- AC:END -->
