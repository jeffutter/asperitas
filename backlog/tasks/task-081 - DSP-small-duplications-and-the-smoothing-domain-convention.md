---
id: TASK-081
title: 'DSP: small duplications and the smoothing-domain convention'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
labels:
  - review-2026-10
  - dsp
dependencies: []
priority: low
type: chore
ordinal: 183800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Smoothing time constants are written twice (10.0 at gain.rs:35-36,47-48; 5.0 at filter.rs:42,55). Gain runs two smoothers that always hold the same target (gain.rs:27-28,57-58). snap_params duplicates reset for Gain (gain.rs:79-82). i16 scaling divides by i16::MAX so -32768 reads -1.00003 (wav_io.rs:31,40-41). The filter smooths the coefficient alpha while Gain smooths linear amplitude (filter.rs:63-66, gain.rs:55-58); for a resonator, smoothing coefficients gives non-uniform glides and can pass through unstable pairs. Decide and document the convention in smooth.rs: smooth the perceptual parameter (log Hz, dB) and derive coefficients per sample or sub-block.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Named consts replace the repeated time constants; Gain has one smoother
- [ ] #2 i16 conversion is symmetric and shared
- [ ] #3 smooth.rs documents the smoothing-domain convention and the filter follows it (goldens regenerated with a listen if they move)
<!-- AC:END -->
