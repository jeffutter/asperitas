---
id: TASK-035.01
title: >-
  Add asperitas-measure crate: capture/stimulus loading and single-place
  thresholds
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:04'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-035
priority: high
ordinal: 162800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
New host-only workspace crate asperitas-measure. Loads a stimulus/capture pair: reassembled PCM from a .console.zst (via asperitas-logging dump reassembly) and the matching stimulus regenerated from asperitas-dsp stimulus processors (Sine, ExponentialSweep, PulseTrain) with the parameters the rig used. Holds one thresholds module (all numeric limits, each with a doc comment citing its origin, e.g. TASK-038.05 baseline: loop gain -0.41 dB, noise+distortion 0.35 LSB RMS, DC -0.49 LSB). Fixtures are the committed audio/captures/*.console.zst, no board needed. AC: crate builds and tests in CI; thresholds exist only in that module; loading 2026-10-07-rig-sine-300s.console.zst yields the expected sample count. Covers TASK-035 AC5, AC6.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 New workspace member crates/asperitas-measure builds, clippy-clean and tests in CI, with [lints] workspace = true
- [x] #2 Capture::from_console_zst loads audio/captures/2026-10-07-rig-sine-300s.console.zst and yields exactly 14401536 mono samples (28803072 bytes PCM / 2) with 879/879 blocks proved
- [x] #3 A capture with any refused/abandoned block or failed frame returns an error, never a silently short sample vector
- [x] #4 Stimulus regeneration from asperitas-dsp Sine, ExponentialSweep and PulseTrain with caller-given params yields a Vec<f32>; a test regenerates the rig sine (1 kHz, -20 dBFS) and checks length and peak
- [x] #5 All numeric limits live only in src/thresholds.rs, each with a doc comment citing its origin (TASK-038.05 baseline: loop gain -0.41 dB, noise+distortion 0.35 LSB RMS, DC -0.49 LSB)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d866717. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-measure: capture.rs (strict reassembly mirroring dump_reassemble, ruzstd decode), stimulus.rs (Sine/Sweep/Pulse render, left lane, reset before render), thresholds.rs. 300 s fixture loads 14401536 samples, 879 blocks. Synthetic round-trip, dropped-chunk and flipped-byte tests. Cargo test, clippy workspace and doc pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New host-only crate asperitas-measure with strict Capture loader, Stimulus renderer and single-place thresholds; all 5 ACs verified by tests.
<!-- SECTION:FINAL_SUMMARY:END -->
