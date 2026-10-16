---
id: TASK-079
title: >-
  DSP: stop processors clamping their own output; one saturating stage at the
  host boundary
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
labels:
  - review-2026-10
  - dsp
dependencies:
  - TASK-077
priority: medium
type: chore
ordinal: 181800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Gain, OnePoleLowPass and all three stimuli hard-clip to +-1 (gain.rs:64-67, filter.rs:73). That makes them nonlinear (Gain +12 dB on 0.5 gives [1,1]), clips between chain stages, and makes the *_output_bounded proptests (property_tests.rs:47-57, 175-185) pass by construction, so they would never see a runaway resonator, the M5 property that matters. encode_block (main.rs:139-140) and wav_io.rs:70-71 already clamp at the boundary.

Change: remove the per-processor clamps, keep the boundary clamps (and non-finite sanitising), make boundedness tests assert a real bound on unclamped output.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No processor in asperitas-dsp clamps its own output; the codec/WAV boundary still saturates
- [ ] #2 Boundedness proptests assert a real bound on unclamped output
- [ ] #3 main's audible behaviour is unchanged (Gain is last in its chain and encode_block clamps identically)
<!-- AC:END -->
