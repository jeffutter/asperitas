---
id: TASK-032.02
title: >-
  Add a knob-boundary override layer with a test proving
  upstream-vs-downstream-of-smoothing differs
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 06:15'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-032
priority: high
ordinal: 156800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a host-testable override layer in asperitas-pod at the knob-value boundary (where main.rs KnobState hands knob readings to the audio callback), strictly upstream of any DSP parameter smoothing. Per knob: Knob(controlled by pot) or Host(value) state; set, release, and a merge function the callback calls. Value clamped to 0..=1 like knob::normalize. A host test using a real asperitas-dsp smoothed parameter shows the step response differs between injecting at the knob boundary (smoothing in circuit) and setting the smoothed parameter directly, satisfying TASK-032 AC #3. A test shows that with no override active the merged value is bit-identical to the knob reading (AC #4). Out of scope: wire protocol, firmware wiring.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 knob_override module in asperitas-pod with per-knob Knob/Host state, set, release and merge, value clamped to 0..=1
- [x] #2 Host test with a real asperitas-dsp Gain shows knob-boundary injection ramps while direct (reset-snapped) setting does not
- [x] #3 Host test shows merge with no override active is bit-identical to the knob reading
- [x] #4 cargo test and clippy for asperitas-pod pass; firmware and wire protocol untouched
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by eea6735. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-pod/src/knob_override.rs (KnobOverride<N>: set/release/release_all/is_active/merge; NaN and out-of-range idx ignored, values clamped 0..=1). asperitas-dsp added as a dev-dependency only (not in normal dep tree). Smoother time constant is 480 samples, so the convergence test runs 6000 samples. Firmware and wire protocol untouched.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Knob-boundary override layer with host tests: bit-identical pass-through with no override, clamping, per-knob independence, and a real Gain test showing boundary injection ramps (monotonic, converges) while a reset-snapped direct set does not. cargo test, clippy, fmt, doc pass.
<!-- SECTION:FINAL_SUMMARY:END -->
