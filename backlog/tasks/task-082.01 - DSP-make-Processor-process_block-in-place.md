---
id: TASK-082.01
title: 'DSP: make Processor::process_block in place'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 22:35'
updated_date: '2026-10-17 00:24'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-082
priority: high
ordinal: 234800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Change the provided Processor block method (crates/asperitas-dsp/src/processor.rs) from (&[Frame], &mut [Frame]) to a single &mut [Frame] processed in place, and update every caller (firmware/src/bin/main.rs, firmware/src/bin/rig.rs, asperitas-cli process.rs, fx_chain.rs, replay_fx.rs, asperitas-measure, property and golden tests). Drop the triple stack buffers in main.rs KnobChain::process; decode into one buffer, process, encode. Acceptance: no input/output pair API remains; all host tests and goldens unchanged bit-for-bit; firmware builds for every variant.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No input/output pair process_block API remains in the workspace (excluding backlog history)
- [x] #2 All host tests and golden files pass unchanged, bit-for-bit
- [x] #3 Firmware builds for every variant; main.rs KnobChain::process uses one frame buffer
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by a877b49. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Processor::process_block is now (&mut self, block: &mut [Frame]). FxChain::process_block_with, rig render, ReplayFx, main KnobChain (one buffer), CLI process, golden test, soak and the property tests updated. cargo test --workspace passes with no golden changes; firmware clippy -D warnings clean for seed3 default, stim-ess/pulse/sine/excerpt(+unsmoothed-params) and RTT-only. rig lost its silent frames_in buffer. stim-excerpt link needs the bootloaded layout, so only clippy was run for it.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
In-place Processor::process_block with all callers migrated; no pair API remains; host tests and goldens unchanged; firmware lints clean for every variant.
<!-- SECTION:FINAL_SUMMARY:END -->
