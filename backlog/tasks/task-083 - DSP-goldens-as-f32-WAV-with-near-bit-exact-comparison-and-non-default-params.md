---
id: TASK-083
title: 'DSP: goldens as f32 WAV with near-bit-exact comparison and non-default params'
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
labels:
  - review-2026-10
  - dsp
  - testing
  - needs-decision
dependencies: []
priority: medium
type: chore
ordinal: 185800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
golden_tests.rs:68-80 runs default params only; Gain at 0 dB is an identity, 11 of 22 goldens are copies of their input (audio/README.md admits it). Output is stored as 16-bit and compared at 1e-4 (~3.3 LSB, golden_tests.rs:24), so a change to a -60 dB resonator tail would be invisible. The CLI dispatch path (process in the binary) is never golden-tested. No analysis subcommand (impulse/frequency response) exists yet, planned for M5.

DECISION NEEDED: regenerating all 22 goldens as f32 WAV needs a listen on each.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Goldens are 32-bit float WAV compared near bit-exact
- [ ] #2 Each processor has at least one non-default-parameter golden
- [ ] #3 Golden cases run through the same entry point the CLI uses
<!-- AC:END -->
