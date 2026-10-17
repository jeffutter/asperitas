---
id: TASK-082.04.01
title: Run firmware main on the Asperitas effect
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 00:38'
updated_date: '2026-10-17 00:41'
labels:
  - planned
dependencies: []
parent_task_id: TASK-082.04
priority: high
ordinal: 238800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace KnobChain's hand-wired OnePoleLowPass and Gain and its params_from_normalised knob mapping in firmware/src/bin/main.rs with asperitas_dsp::Asperitas, fed Controls { knobs } from host_control::merge. Keep decode_block/encode_block as the only buffer (no triple buffering: one Frame array, in place). Acceptance: knob mapping lives only in asperitas-dsp; main.rs has no filter/gain imports; all firmware variants build (cargo build for each bin/feature set the host gate uses) and the host gate passes. max_block_us recheck is deferred to TASK-120 (HUMAN).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 84e88a3. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
main.rs now owns one asperitas_dsp::Asperitas; knob mapping removed from firmware. Verified with scripts/gates.sh commit (17 gates pass). max_block_us recheck deferred to TASK-120.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
firmware/src/bin/main.rs replaces the hand-wired OnePoleLowPass and Gain with asperitas_dsp::Asperitas, fed Controls { knobs } from host_control::merge; decode/encode remain the single in-place buffer. No filter/gain imports or params_from_normalised left in main.rs.
<!-- SECTION:FINAL_SUMMARY:END -->
