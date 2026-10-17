---
id: TASK-080
title: 'DSP: one generic processor-laws proptest harness'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-17 01:03'
labels:
  - review-2026-10
  - dsp
  - testing
  - planned
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
- [x] #1 A generic harness runs the same laws for Gain and OnePoleLowPass with arbitrary params and differing prior histories
- [x] #2 The copied per-processor property tests are deleted
- [x] #3 The FxChain::reset contract deviation is documented and tested
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Harness: processor_laws! macro in crates/asperitas-dsp/tests/property_tests.rs over generic law_* fns (finite+bounded with per-effect bound closure, silence, block==tick, reset after two different histories and params bit-identical, param step max per-sample delta over a whole 997 Hz sine window vs steady slews plus gap/20, NaN/inf/huge input and hostile params with recovery). Instantiated for Gain and OnePoleLowPass; copied gain_*/filter_* tests and the run_finite_after_event tests deleted; call-order, exact-zero-decay and Gain tail tests kept. Mutation-checked: removing Gain snap, filter state clear, and smoothing each fail the matching law. Finding: huge finite input times the 8192 gain ceiling overflows to inf (output is unclamped by design), so the hostile law asserts no NaN, then finite after recovery. Finding: filter cutoff 0 Hz freezes state rather than muting, so it sits in the hostile strategy, not the silence one. FxChain::reset doc now states it deliberately breaks the Processor reset law; new unit test reset_restarts_the_generator_but_not_the_effect.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Generic processor-laws proptest harness replaces the per-processor copies for Gain and OnePoleLowPass, with stronger reset, param-step and hostile-input laws; FxChain::reset deviation documented and tested.
<!-- SECTION:FINAL_SUMMARY:END -->
