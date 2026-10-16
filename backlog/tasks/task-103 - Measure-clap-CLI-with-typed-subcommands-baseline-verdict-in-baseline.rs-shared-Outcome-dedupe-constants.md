---
id: TASK-103
title: >-
  Measure: clap CLI with typed subcommands, baseline verdict in baseline.rs,
  shared Outcome, dedupe constants
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - measure
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
- [ ] #1 Typed subcommands replace the hand-rolled parser; tests/cli.rs passes
- [ ] #2 One definition each of the sample rate, full scale, Outcome
- [ ] #3 require_status removed
<!-- AC:END -->
