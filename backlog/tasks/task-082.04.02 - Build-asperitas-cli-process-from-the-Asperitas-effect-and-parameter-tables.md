---
id: TASK-082.04.02
title: Build asperitas-cli process from the Asperitas effect and parameter tables
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 00:38'
updated_date: '2026-10-17 01:07'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-082.04
priority: medium
ordinal: 239800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-cli/src/process.rs hand-matches 'gain' and 'filter' and builds each processor from its param table. Make it derive from the shared tables/Asperitas effect: keep per-stage processors selectable if useful, and add the full chain (e.g. processor 'asperitas' taking cutoff_hz and gain_db) built via Asperitas::new / Controls, with no duplicate table-walking logic. Acceptance: process supports the full effect chain; parameter parsing stays table-driven; existing cli tests pass or are updated; host gate passes.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-082.04.02. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ParamSpec::to_normalised (inverse of from_normalised, clamped, NaN -> default); Controls::default now uses it. process.rs gains the 'asperitas' processor built from FILTER_PARAMS ++ GAIN_PARAMS via parse_with_table and Asperitas::new; gain/filter remain selectable. Tests: round trip, clamp/NaN, asperitas equals filter-then-gain.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-cli process now supports the full effect chain via processor 'asperitas' (cutoff_hz, gain_db), table-driven, built through Asperitas::new/Controls.
<!-- SECTION:FINAL_SUMMARY:END -->
