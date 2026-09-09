---
id: TASK-035
title: 'Add loopback measurement analysis: stimulus, alignment, and objective metrics'
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:26'
updated_date: '2026-09-09 02:13'
labels: []
dependencies:
  - TASK-031
  - TASK-034
  - TASK-038
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
The bench from TASK-034 closes the analog loop and TASK-038 puts stimulus through it and brings the result back; the runner from TASK-031 makes the control-state side trustworthy. What is missing is the analysis that turns samples into a verdict, without which the rig is only a more convenient way to take notes by hand.

Set expectations precisely, because getting this wrong produces confident nonsense. Two things changed when the measurement moved from an external interface onto the device itself. Clock drift disappeared — one codec, one MCLK — so alignment should assert that the offset is stable rather than tolerating slip, which is a stronger claim and a cheap regression tripwire. But comparing a device against itself means the analog path being measured includes both of its own converters, so a figure describing "the output stage" is really describing output plus input plus whatever they share. Report against the baseline captured in TASK-034 and state what a deviation can and cannot be attributed to.

The fault this arrangement cannot see is one common to both directions — a shared clock misconfiguration, or a channel swap inside the capture code. That blind spot is handled by TASK-034 criterion #2, permanently human, and not by anything here. Do not let a green suite imply that hole is covered.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Analysis consumes a stimulus/capture pair produced by TASK-038 and reports the cross-correlation offset per window; repeating the capture shows the same offset, confirming the single-clock-domain assumption rather than assuming it.
- [ ] #2 Each capture reports machine-readable metrics — peak and RMS level, runs of silence, sample-discontinuity candidates with counts and positions, and a spectral spur report — normalized against the loop gain recorded in TASK-034.
- [ ] #3 A parameter-step detector passes an intentionally unsmoothed parameter ramp and rejects the smoothed one, demonstrating the detector has sensitivity rather than merely never firing.
- [ ] #4 Device-through-loopback output agrees with asperitas-cli output over the same QSPI-stored excerpt at matched parameters within a stated tolerance, and the reason bit-exactness is not claimed is written down so nobody later loosens the tolerance to make a test pass.
- [ ] #5 Thresholds live in one place and tests assert on the metrics, so a ticket can fail on a number rather than an opinion.
- [ ] #6 Unit tests run on recorded fixtures with no board and no audio hardware attached, so CI stays green without the bench.
- [ ] #7 Usage is documented alongside the other host tooling.
<!-- AC:END -->
