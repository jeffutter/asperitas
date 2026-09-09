---
id: TASK-035
title: 'Add loopback measurement analysis: stimulus, alignment, and objective metrics'
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:26'
updated_date: '2026-09-09 01:31'
labels: []
dependencies:
  - TASK-031
  - TASK-034
references:
  - 'https://github.com/daisy-embassy/daisy-embassy/pull/80'
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - crates/asperitas-rig/src/main.rs
  - audio/goldens/
priority: high
type: feature
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The bench from TASK-034 can play and record; the runner from TASK-031 can capture trustworthy control state. What is missing is the analysis that turns a recording into a verdict, without which the rig is only a more convenient way to take notes by hand.

Set expectations precisely, because getting this wrong produces confident nonsense. Byte-exact comparison between the device path and asperitas-cli is not achievable through two converters and two independent clocks. The target is statistical agreement inside a stated tolerance, and the reason bit-exactness is abandoned must be written down so nobody later "fixes" the tolerance upward to make a test pass.

Cheap conversion bounds what may be claimed. Gross correctness, glitches, dropouts, level and monotonicity are all admissible; absolute noise-floor claims are not, since the bench's own distortion and noise will dominate the TAC5242's. Metrics should therefore be reported relative to the direct-loopback baseline captured in TASK-034, not as absolute figures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deterministic stimuli are generated — fixed-level sine, swept sine, impulse train — and played and recovered through the bench against a known reference.
- [ ] #2 Captured audio is aligned to stimulus by per-window cross-correlation, tolerating the drift measured in TASK-034 without assuming a fixed offset.
- [ ] #3 Each capture reports machine-readable metrics: peak and RMS level, runs of silence, sample-discontinuity candidates with counts and positions, and a spectral spur report.
- [ ] #4 A parameter-step detector passes a deliberately unsmoothed parameter ramp and rejects the smoothed one, demonstrating the detector has sensitivity rather than merely never firing.
- [ ] #5 Device-through-loopback output is compared against asperitas-cli output over files from audio/instruments/ at matched parameters, reporting an agreement metric within a stated tolerance.
- [ ] #6 Thresholds live in one place and tests assert on the metrics, so a ticket can fail on a number rather than an opinion.
- [ ] #7 Unit tests run on recorded fixtures with no board and no audio hardware attached, so CI stays green without the bench.
- [ ] #8 Usage is documented alongside the other host tooling.
<!-- AC:END -->
