---
id: TASK-027
title: >-
  HUMAN: Capture podtest run and confirm ~1 kHz poll rate (TASK-025 AC #3 never
  verified)
status: Done
assignee:
  - '@human'
created_date: '2026-08-09 05:08'
updated_date: '2026-10-15 05:55'
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
- [x] #1 Both subtasks are done: TASK-027.01 (flash podtest and leave the bench rigged for scripted capture) and TASK-027.02 (measure the achieved poll rate and check for catch-up bursts from a scripted capture).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Restructured 2026-09-09 into an umbrella with two subtasks. The original four criteria were measurements — take a capture, compute a median interval, check for bursts, report the number — carrying HUMAN: prefixes only because no unattended capture path existed. TASK-030 and TASK-031 create that path, so the measurement moved to TASK-027.02 where an agent can carry it, and what genuinely needs hands (flashing over DFU, leaving the bench cabled) stayed as TASK-027.01.

This ticket stays @human under the parent-inherits-strictest rule and cannot close until TASK-027.01 does. The measurement criteria themselves no longer ask a person to do arithmetic.

2026-10-15 measurement (TASK-027.02): captures/task-027-02-podtest-knob.txt, 250 s via asperitas-rig (bad_frames=0, accounting_ok=true, 25360 records, one leading gap seq 0x25->0x1bb with device_dropped=405 and 49 discarded bytes from pre-attach, so rig verdict=fail on discarded_bytes only; stats use records after the gap). Knob interval from body t= over 24828 deltas (scripts/knob-interval.py): median 10.0 ms, mean 10.071, p5 10, p95 11, min 10, max 11, stdev 0.257 ms (1 ms quantisation). No deltas <=2 ms, no stalls >20 ms, so no catch-up bursts. Controls assumed untouched. Consistent with true 1 kHz poll rate.

2026-10-16: Closed with the owner. A second, independent measurement from the TASK-033 session (captures/task-033-controls-270s.txt, 270 s, encoder and buttons in use): 26814 knob deltas, median 10.0 ms, mean 10.071, stdev 0.257, p5 10, p95 11, no delta <=2 ms, none >20 ms (scripts/knob-interval.py). It agrees with the 2026-10-15 number, so the ~1 kHz claim is hardware-verified; the owner agreed.
<!-- SECTION:NOTES:END -->
