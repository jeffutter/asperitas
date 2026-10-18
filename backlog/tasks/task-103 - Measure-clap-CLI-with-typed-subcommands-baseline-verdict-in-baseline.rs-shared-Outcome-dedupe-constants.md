---
id: TASK-103
title: >-
  Measure: clap CLI with typed subcommands, baseline verdict in baseline.rs,
  shared Outcome, dedupe constants
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 23:40'
labels:
  - review-2026-10
  - measure
  - planned
dependencies: []
priority: low
type: chore
ordinal: 205800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
measure's CLI is hand-rolled (cli.rs:149-253, ~100 lines with ~10 manual exclusivity checks); Mode::Stimulus(String) is re-matched later (cli.rs:283,405); --sweep-start-s is reused to mean the soak start; the --expect-fail verdict is read back out of serialized JSON (cli.rs:438) instead of SoakReport; the sine-baseline judgment lives in cli.rs:282-351 while sweep/soak/compare each have a module and overlap soak.rs; Outcome is duplicated with rig cli.rs:14. SAMPLE_RATE_HZ is defined three times (capture.rs:24, stimulus.rs:9, logging capture), FULL_SCALE twice (metrics.rs:34, compare.rs:32, literal 32768.0 at cli.rs:307,377 and soak.rs:157), i16-to-f32 normalisation written three times, align(&Capture) makes compare.rs:219-224 copy the head into a fake Capture, and a 4096 literal at metrics.rs:275 breaks thresholds.rs's no-literal rule. Policy::require_status (rig lib.rs:98) is true at both production call sites and false by default.

Change: clap subcommands analyze baseline|excerpt|sweep|soak with typed enums; move judge_stimulus into baseline.rs returning a typed report; share Outcome; Capture::normalized(); align(&[i16]); one rate constant; remove Policy::require_status (always require STATUS). tests/cli.rs pins behaviour.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Typed subcommands replace the hand-rolled parser; tests/cli.rs passes
- [x] #2 One definition each of the sample rate, full scale, Outcome
- [x] #3 require_status removed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-103. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
CLI kept the pinned shape: clap derive Cli with one Analyze subcommand, StimulusKind ValueEnum (clap rejects unknown stimuli), conflicts_with for stimulus/excerpt and excerpt/expect-fail. sweep/soak/hold stay sidecar-selected, so no baseline|excerpt|sweep|soak subcommands (they would break tests/cli.rs and rig docs). Sidecar::sweep_start_s renamed motion_start_s. judge_stimulus moved to baseline.rs as judge_baseline returning typed BaselineReport. --expect-fail reads the verdict from SoakReport via Judged. Outcome now lives in asperitas-host::outcome (rig re-exports it). SAMPLE_RATE_HZ is asperitas_wire::capture's, FULL_SCALE is capture::FULL_SCALE, Capture::normalized() replaces the i16-to-f32 maps, align takes &[i16], the 4096 literal is thresholds::FIXED_GAIN_BLOCK_SAMPLES. Policy::require_status removed; NoStatus is now always a failure, so two rig tests (artifact golden, Acker test reader) were updated to emit/expect STATUS. analyze JSON on the sine fixture is byte-identical before and after.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
measure CLI moved to clap derive with typed StimulusKind, baseline verdict in baseline.rs, shared Outcome in asperitas-host, single sample-rate/full-scale constants, Capture::normalized, align(&[i16]), and rig Policy::require_status removed. Workspace tests, clippy and fmt green; analyze output on the sine fixture unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
