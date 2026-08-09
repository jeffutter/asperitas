---
id: TASK-018.04
title: Verify every Pod control on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-05 17:27'
updated_date: '2026-08-09 04:42'
labels: []
dependencies:
  - TASK-018.01
  - TASK-018.02
  - TASK-018.03
  - TASK-020
  - TASK-021
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board, the Pod, and hands. Nothing in TASK-018.01 through TASK-018.03 is confirmed by a build succeeding — a pin map can compile perfectly and address the wrong pins, and an encoder can count backwards without a single warning.

Needs a firmware binary that surfaces control state. USB CDC serial logging already works (TASK-006), so a `podtest` bin in the mould of firmware/src/bin/ledtest.rs — streaming knob values and control events over serial — is the natural harness. Building that harness is part of this ticket.

Two observations matter more than the rest, because they are the ones that cost time downstream if wrong:

- KNOB JITTER MAGNITUDE. A knob held still should not wander. Record the actual peak-to-peak wobble, in ADC counts or normalised units, so TASK-019 knows whether the smoothing from TASK-018.02 is good enough before a knob drives an audio parameter. A number here is worth far more than "looked stable".

- ENCODER DIRECTION AND DETENT RATIO. Confirm clockwise is positive and that one physical detent produces exactly one increment, in both directions. Both errors are invisible to code review and obvious in the hand.

Also confirm the LED boundary held: LED 1 must still show its boot and panic stages, unchanged. If asperitas-pod disturbed asperitas-logging LED ownership, this is where it shows up.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: both knobs sweep smoothly across the full 0.0 to 1.0 range, reaching the endpoints at their physical stops
- [ ] #2 HUMAN: a knob held still holds its value; the observed peak-to-peak jitter is recorded as a number in the implementation notes
- [ ] #3 HUMAN: the encoder produces one increment per physical detent in both directions, clockwise positive
- [ ] #4 HUMAN: the encoder click and both buttons each register exactly one event per press, with no double-fires
- [ ] #5 HUMAN: LED 2 shows the expected colour for each channel combination, confirming the active-low drive
- [ ] #6 HUMAN: LED 1 still shows its boot and panic stages — asperitas-pod has not disturbed asperitas-logging LED ownership
- [ ] #7 Any correction to the pin map, the LED polarity, or the drive approach is written back into docs/reference/daisy-pod.md
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Hardware capture 2026-08-08 (podtest with slow-boot, 13968 lines / 240 s, protocol run by hand). Findings per AC — checkboxes deliberately left unticked, this ticket is @human.

AC #1 — PASS. Both knobs reach raw 0 and 65535 exactly (normalised 0.0000 / 1.0000) at their physical stops. Slow sweeps are continuous: knob 1 travels 65530 -> 4 across ~300 samples over 6 s with no plateaus. Only true after the TASK-023 normalisation fix; the pre-fix capture showed three usable values across an entire sweep.

AC #2 — PASS. JITTER NUMBER, measured on settled hold windows of 500+ samples each:

  knob 1 @ mid-travel   n=514  sigma=2.15  peak-to-peak 14 counts
  knob 1 @ stop         n=566  sigma=1.48  peak-to-peak  8 counts
  knob 2 @ mid-travel   n=565  sigma=1.97  peak-to-peak 11 counts
  knob 2 @ stop         n=566  sigma=0.93  peak-to-peak  5 counts

Worst observed peak-to-peak is 14 counts out of 65535 = 0.021% of full scale. Over multi-minute windows it reaches 39 counts (0.06% FS) including slow drift. The distribution is clean Gaussian, no popcorn noise.

FOR TASK-019: the hardware averaging from TASK-018.02 (Averaging::Samples16) is more than sufficient. No EMA or additional smoothing is needed before a knob drives an audio parameter — 0.02% of a parameter range is inaudible.

AC #3 — FAILS on the ratio, passes on direction. Clockwise IS positive (correct). But one physical detent produces FOUR increments: 10 deliberate clockwise detents netted +40, 10 counter-clockwise netted -38. Per-detent clusters [4,4,3,4,4,3,4,4,4,4,4] and [-4,-4,-3,-4,-4,-4,-3,-4,-3,-4]. The Pod encoder rests at every fourth quadrature state. Tracked as TASK-024; this AC cannot pass until that lands and the encoder steps are re-run.

Brisk-spin segments produced +86 and -90 against roughly 21-22 actual detents (the tester could not gauge exactly 20). Counts came out HIGH, not low, so there is no evidence of detent-dropping at speed — TASK-021 concern appears unfounded.

AC #4 — PASS. Encoder click: exactly 5 press + 5 release, clean, no double-fires. BTN1 and BTN2 each LOGGED 4 press + 5 release, but both missing presses sit immediately after a truncated log line, and DebouncedSwitch can only emit Release from an internally-pressed state — so those presses were emitted and the USB CDC transport lost them. 5/5 on all three controls, no double-fires anywhere.

AC #5 — PASS. Observed order black, red, green, blue, yellow, light blue, purple, white, repeat — an exact match for Off, Red, Green, Blue, Yellow, Cyan, Magenta, White. Active-low drive confirmed on LED 2.

AC #6 — PASS. Boot: LED 1 holds red a few seconds then goes steady green (slow-boot build). Panic: green for a while, then solid red. Both stages intact, so asperitas-pod has not disturbed asperitas-logging LED ownership.

AC #7 — DONE. docs/reference/daisy-pod.md updated in commit 5e169d3 with three sections: the STM32H7 16-bit ADC reset-value trap, knob rotation direction (clockwise increases — this CORRECTED an earlier note that recorded the opposite as a suspicion, drawn from a capture taken through the normalisation bug), and the encoder 4:1 detent ratio.

TWO DEFECTS FOUND OUTSIDE THE ACs:
- Poll loop runs at 625 Hz, not the ~1 kHz ControlSurface contract. Timer::after_millis sleeps AFTER the work, so the period is work + 1 ms. Tracked as TASK-025.
- ~8.8% of log lines are truncated over USB CDC (1226 of 13968, directly counted). info! is not atomic against the CDC buffer. This ate two button-press lines and corrupted knob samples — one line read r2=298 mid-number, which nearly got reported as an ADC glitch. NOT YET TICKETED. It degrades every future hardware capture, which is this project's only verification mechanism.

BLOCKING THIS TICKET: AC #3 only. All other criteria are satisfied.
<!-- SECTION:NOTES:END -->
