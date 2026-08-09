---
id: TASK-024
title: >-
  Fix: EncoderDecoder emits 4 increments per physical detent — the Pod encoder
  is detented every fourth quadrature state
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-09 04:32'
labels:
  - review-followup
dependencies:
  - TASK-018.03
  - TASK-018.04
documentation:
  - docs/reference/daisy-pod.md
priority: high
type: bug
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measured on hardware 2026-08-08 from a podtest capture (~/podtest.log, 240 s). Ten deliberate clockwise detents produced a net +40; ten counter-clockwise detents a net -38. Per-detent transition clusters were [4,4,3,4,4,3,4,4,4,4,4] and [-4,-4,-3,-4,-4,-4,-3,-4,-3,-4]. The Pod's encoder rests at every FOURTH quadrature state, so one physical detent walks the full 00->01->11->10 Gray cycle. ENCODER_LUT in crates/asperitas-pod/src/encoder.rs emits +/-1 on every valid transition, so ControlSurface reports four increments per click.

Direction is CORRECT: clockwise is positive. Only the ratio is wrong. This is the residual failure of TASK-018.04 AC #3 ('the encoder produces one increment per physical detent in both directions, clockwise positive').

ControlEvent::EncoderDelta's own doc comment says 'signed detent increment', so the contract is detents and the implementation delivers quarter-detents. Every downstream consumer (TASK-019 parameter mapping) would be off by 4x.

DESIGN CONSTRAINT — the divide-by-four must CARRY THE REMAINDER, not truncate per poll. Three of the ten counter-clockwise detents registered 3 transitions rather than 4 (contact bounce the LUT correctly filters, plus transitions arriving closer together than the poll period). A per-poll 'delta / 4' discards those clusters as zero, turning a cosmetic ratio bug into dropped detents. An accumulator that emits one detent per 4 accumulated quarter-steps and retains the remainder does not.

Consider also the more robust 'full-step' variant used for detented encoders: emit a detent only on arrival at the rest state, using accumulated direction. That tolerates a missing intermediate transition outright rather than letting the accumulator phase-slip. Confirm the actual rest state from hardware before choosing it.

Note this is separable from TASK-025 (poll rate): the 4:1 ratio is present at any poll rate. The brisk-spin segments of the same capture produced +86 and -90 against ~21-22 actual detents, i.e. counts came out HIGH not low, so there is no evidence of mass detent-dropping at 625 Hz.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 one physical detent produces exactly one EncoderDelta increment in both directions, clockwise positive
- [ ] #2 the quarter-step-to-detent conversion carries its remainder rather than truncating per poll, so a detent that registers only 3 transitions is not silently discarded
- [ ] #3 host-side unit tests in crates/asperitas-pod/src/encoder.rs drive the decoder with the ACTUAL transition sequences captured from hardware (including the 3-transition clusters) and assert the detent counts, not a re-implementation of the formula
- [ ] #4 HUMAN: a fresh podtest capture shows 10 deliberate detents producing a net +10 clockwise and -10 counter-clockwise
- [ ] #5 docs/reference/daisy-pod.md's encoder detent-ratio section is updated to record the fix
<!-- AC:END -->
