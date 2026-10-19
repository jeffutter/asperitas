---
id: TASK-140
title: 'Fix: restore sweep-shape/level coverage and verify goldens after stimulus move'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 17:22'
updated_date: '2026-10-19 17:41'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 266800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Commit 7131e5a deleted crates/asperitas-cli/tests/stimulus_shape_tests.rs (device vs golden sweep shape, dBFS level scaling, fades-only difference) and changed synth.rs: sample_count now rounds instead of truncating (0.1 s at 44.1 kHz went 4409 -> 4410) and generate_sweep builds Chirp from N/fs with different phase grouping. No goldens were checked against the old implementation. Re-establish equivalent coverage and confirm goldens still pass.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-140. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Goldens verified unchanged at HEAD (cli 15 golden tests, stimulus 23 + 15 property pass); no regeneration needed. Restored coverage in asperitas-stimulus/tests/stimulus_tests.rs: one-second 48k case at 1e-6 interior tolerance, -20 dBFS level scaling, fades-only disagreement. Added generate_sweep(48000,1.0) pin test in asperitas-cli synth.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Goldens pass unchanged after the stimulus move. Restored sweep shape, dBFS level and fades-only coverage in asperitas-stimulus tests (1e-6 tolerance) and added a generate_sweep endpoint/length test in asperitas-cli. Gates commit tier passes.
<!-- SECTION:FINAL_SUMMARY:END -->
