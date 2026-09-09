---
id: TASK-027
title: >-
  HUMAN: Capture podtest run and confirm ~1 kHz poll rate (TASK-025 AC #3 never
  verified)
status: To Do
assignee:
  - '@human'
created_date: '2026-08-09 05:08'
updated_date: '2026-09-09 01:35'
labels:
  - review-followup
dependencies:
  - TASK-025
  - TASK-026
documentation:
  - docs/reference/daisy-pod.md
priority: high
ordinal: 110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-025 fixed podtest's poll loop and firmware/src/bin/main.rs's knob_poll_task to use embassy_time::Ticker instead of Timer::after_millis, eliminating the drift that produced ~625 Hz instead of the ~1 kHz ControlSurface contract. TASK-025's own AC #3 ("HUMAN: a fresh podtest capture shows a knob log line interval of ~10 ms (KNOB_LOG_THROTTLE=10 at 1 kHz), confirming the achieved rate rather than the nominal one") was correctly left unchecked — it requires the board and a hand-run capture, which no agent can do — but TASK-025 was marked Done anyway with no follow-up ticket tracking the still-missing verification. This ticket closes that gap: it is the standing record that the ~1 kHz claim is code-verified (build/clippy) but not yet hardware-verified.

Wait for TASK-026 (Ticker catch-up-burst bound) to land first, since that changes the exact poll timing this capture measures — capturing against the pre-TASK-026 code would need to be redone anyway.

Correct axis: per this project's CLAUDE.md, "If a criterion says HUMAN:, no amount of agent work satisfies it" — this ticket is the mechanism that actually closes TASK-025 AC #3 rather than leaving it permanently unchecked on a Done ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both subtasks are done: TASK-027.01 (flash podtest and leave the bench rigged for scripted capture) and TASK-027.02 (measure the achieved poll rate and check for catch-up bursts from a scripted capture).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Restructured 2026-09-09 into an umbrella with two subtasks. The original four criteria were measurements — take a capture, compute a median interval, check for bursts, report the number — carrying HUMAN: prefixes only because no unattended capture path existed. TASK-030 and TASK-031 create that path, so the measurement moved to TASK-027.02 where an agent can carry it, and what genuinely needs hands (flashing over DFU, leaving the bench cabled) stayed as TASK-027.01.

This ticket stays @human under the parent-inherits-strictest rule and cannot close until TASK-027.01 does. The measurement criteria themselves no longer ask a person to do arithmetic.
<!-- SECTION:NOTES:END -->
