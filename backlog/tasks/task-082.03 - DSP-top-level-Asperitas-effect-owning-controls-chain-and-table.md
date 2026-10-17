---
id: TASK-082.03
title: 'DSP: top-level Asperitas effect owning controls, chain and table'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 22:35'
updated_date: '2026-10-17 00:35'
labels:
  - planned
dependencies:
  - TASK-082.01
  - TASK-082.02
parent_task_id: TASK-082
priority: high
ordinal: 236800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an Asperitas processor in asperitas-dsp with set_controls(&Controls{knobs:[f32;N],..}) that owns the filter-then-gain chain, maps controls to parameters via the parameter tables, and processes blocks in place. No soft-clip stage (owner decision: hard clamp at codec boundary stays). Host tests: chain equals the manual filter->gain composition bit-for-bit for sample knob settings, NaN/inf robustness, determinism after reset. Depends on the in-place block and parameter-table subtasks. No generic plugin graph.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Asperitas processor in asperitas-dsp owns filter-then-gain chain and maps Controls knobs via the parameter tables, in place, no soft-clip
- [x] #2 Host tests: bit-for-bit equality with manual filter->gain composition, NaN/inf robustness, determinism after reset
- [x] #3 cargo test and clippy pass for asperitas-dsp, including the no_std build
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 0b18714. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-dsp/src/asperitas.rs: Controls, Asperitas (filter then gain, in place, no soft-clip), set_controls via parameter tables; NaN knobs fall back to the table default. Host tests (need --features std, as the crate's existing tests do) cover bit-exact composition, non-finite robustness, reset determinism, sample rate, defaults. Test, clippy and no_std check pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Asperitas effect added to asperitas-dsp with host tests; firmware wiring is left to TASK-082.04.
<!-- SECTION:FINAL_SUMMARY:END -->
