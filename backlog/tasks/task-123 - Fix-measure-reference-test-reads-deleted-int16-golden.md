---
id: TASK-123
title: 'Fix: measure reference test reads deleted int16 golden'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 01:16'
updated_date: '2026-10-18 22:08'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 241800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-measure/src/compare.rs test reference_matches_the_installed_excerpt_and_the_cli_golden reads audio/goldens/gain_default_mandolin_single_note_soft.wav, deleted when goldens moved to f32. cargo test -p asperitas-measure panics with NotFound. Point it at a surviving f32 golden and decode as f32 (different header/frame size).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 18bd8cf. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added golden_gain_default_mandolin_single_note_soft (f32) and pointed the measure test at it, decoding f32 with a 1e-6 tolerance. measure and cli tests, clippy pass. No acceptance criteria were defined.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Restored the gain default golden for mandolin_single_note_soft as f32 and fixed the measure reference test to decode it. Commit 18bd8cf.
<!-- SECTION:FINAL_SUMMARY:END -->
