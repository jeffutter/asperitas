---
id: TASK-035.05
title: >-
  Compare device-through-loopback output with asperitas-cli over the same QSPI
  excerpt, with documented tolerance
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:43'
labels:
  - planned
dependencies:
  - TASK-035.02
  - TASK-035.03
parent_task_id: TASK-035
priority: high
ordinal: 166800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Compare a capture of an excerpt replayed through the loop (EXCPLAY captures, e.g. 2026-10-15-rig-excerpt-replay-mand-soft) with asperitas-cli process output over the matching audio/goldens/excerpt WAV at matched parameters, after alignment and loop-gain normalization. Tolerance is stated in the thresholds module and the reason bit-exactness is not claimed (two converters, analog noise floor ~16-bit, loop gain, DC offset) is written next to it. RISK: verify whether rig.rs currently runs a processor on the device path; if only passthrough, compare at identity/gain parameters and file a follow-up for processor-on-device. Covers TASK-035 AC4. Depends on alignment and metrics.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Passthrough premise is verified and recorded: firmware/src/bin/rig.rs replay path has no processor, so the comparison runs at identity
- [x] #2 A host function in asperitas-measure (compare.rs) loads the EXCPLAY capture, aligns it to the excerpt, loop-gain normalizes it and returns residual RMS/peak in LSB and dB relative to the reference
- [x] #3 The reference is the asperitas-cli process output (gain at 0 dB) over the matching audio/goldens WAV, produced via the same library code path, not a shell-out
- [x] #4 Tolerance constants live in thresholds.rs with the reason bit-exactness is not claimed written beside them (two converters, ~16-bit analog floor, loop gain, DC offset)
- [x] #5 A test over the committed 2026-10-15-rig-excerpt-replay-mand-soft capture passes within tolerance, and a negative test (wrong excerpt or time-shifted reference) fails it
- [x] #6 Follow-up TASK-035.07 exists for processor-on-device
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by af25d1a. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Premise verified: firmware/src/bin/rig.rs excerpt_play hands PCM straight to the DAC encoder, no processor, so comparison is at identity (gain 0 dB); processor-on-device is TASK-035.07. Excerpt is audio/instruments/mandolin_single_note_soft.wav, CRC 15fd matching EXCOK/EXCPLAY; reference left lane equals committed gain_default golden exactly. Alignment on the 3 windows the excerpt covers (delay 81 samples), then LS gain/DC fit. Measured: loop gain -0.419 dB, DC 0.43 LSB, residual 26.0 LSB RMS (-30.9 dB), peak 803 LSB (-21.5 dB, at the attack). Residual scales with signal (converter filter response), so limits are dB-relative: -25 / -15. Note a uniform time shift of the reference is absorbed by alignment; negative tests use wrong excerpt, mid-excerpt splice (slip) and capture dropout.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-measure compare.rs (Reference::from_excerpt_wav, compare, Comparison::check) and COMPARE_RESIDUAL_RMS_DB/PEAK_DB in thresholds.rs with the not-bit-exact rationale. Replay capture passes (-30.9 dB RMS); wrong excerpt, splice and dropout fail. TASK-035.07 covers processor-on-device.
<!-- SECTION:FINAL_SUMMARY:END -->
