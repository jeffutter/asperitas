---
id: TASK-083
title: 'DSP: goldens as f32 WAV with near-bit-exact comparison and non-default params'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-17 01:13'
labels:
  - review-2026-10
  - dsp
  - testing
  - planned
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
- [x] #1 Goldens are 32-bit float WAV compared near bit-exact
- [x] #2 Each processor has at least one non-default-parameter golden
- [x] #3 Golden cases run through the same entry point the CLI uses
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by e1b84a0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Owner asked why f32. Reason: 16-bit goldens at 1e-4 cannot see changes below ~-90 dB, e.g. a quiet resonator tail, and half the goldens are identity copies. Revisit when the resonator (M5) exists; leave in Backlog until then.

Owner decision 2026-10-16: do it now, not at M5.

Implemented: wav_io writes 32-bit float stereo unclamped and reads 16-bit PCM or 32-bit float; process is now a pub lib module and golden tests call run_process (the CLI entry point). Tolerance 1e-6. Goldens: 14 files (22 old 16-bit files removed), all regenerated; new non-default goldens: gain_cut12_pluck, gain_boost6_sweep, gain_boost12_octave_mandolin_single_note_hard, filter_cutoff500_impulse, filter_cutoff500_sweep, filter_cutoff3000_mandolin_chord, asperitas_cutoff800_cut6_impulse, asperitas_cutoff2000_boost6_mandolin_fast_run, asperitas_default_pluck. Dropped identity gain-default copies (kept gain_default_pluck). Sensitivity check: cutoff 500 -> 500.05 fails filter_cutoff500_impulse (delta 6e-6). Audible review of new goldens is left to the owner. Goldens dir 9.5M -> 7.4M.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Goldens are now 32-bit float WAVs compared at 1e-6, produced through process::run_process, with non-default cases for gain, filter and asperitas.
<!-- SECTION:FINAL_SUMMARY:END -->
