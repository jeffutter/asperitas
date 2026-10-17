---
id: TASK-082.02
title: 'DSP: declarative parameter table per processor'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 22:35'
updated_date: '2026-10-17 00:30'
labels:
  - planned
dependencies: []
parent_task_id: TASK-082
priority: high
ordinal: 235800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Give Gain and OnePoleLowPass one const parameter table (name, unit, range, taper, default) in asperitas-dsp, no_std. Derive from it: params_from_normalised (knob mapping), key=value parsing (the parse function moves to asperitas-cli and consumes the table; remove parse_params_from_cli and the std-gated String code from gain.rs/filter.rs so dsp needs no std for it), and proptest strategies in tests/property_tests.rs. Honour owner decision: block-rate modulation through existing smoothers (no per-sample modulation inputs). Acceptance: tables exist and are the only source of ranges/tapers; CLI parsing, knob mapping and proptests all read them; dsp builds without the std feature for this purpose; goldens unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Const ParamSpec tables for Gain and OnePoleLowPass are the only source of ranges/tapers/defaults
- [x] #2 Knob mapping, CLI parsing (in asperitas-cli) and proptest strategies all read the tables; parse_params_from_cli and std-gated String code removed from asperitas-dsp
- [x] #3 asperitas-dsp builds without std feature; goldens unchanged; cargo test --workspace passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by e063fc7. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added asperitas-dsp/src/param.rs (ParamSpec, Taper) and GAIN_PARAMS/FILTER_PARAMS. params_from_normalised, Default, CLI parse_with_table (asperitas-cli/process.rs) and proptest ranges read the tables. Removed parse_params_from_cli and std String code from dsp. Formulas are bit-identical; goldens and cargo test --workspace pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Parameter tables are the single source of ranges/tapers/defaults; knob mapping, CLI parsing and proptests consume them; dsp has no std-gated code.
<!-- SECTION:FINAL_SUMMARY:END -->
