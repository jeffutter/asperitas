---
id: TASK-027
title: >-
  HUMAN: Capture podtest run and confirm ~1 kHz poll rate (TASK-025 AC #3 never
  verified)
status: To Do
assignee:
  - '@human'
created_date: '2026-08-09 05:08'
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
- [ ] #1 HUMAN: a fresh podtest capture (same protocol as the 2026-08-08 captures referenced in TASK-018.04 and TASK-025 — flash podtest, let it run, record USB CDC serial output) shows a knob log line interval of ~10 ms (KNOB_LOG_THROTTLE=10 at a true 1 kHz poll rate), not the ~16 ms (625 Hz) measured before TASK-025
- [ ] #2 HUMAN: the capture also confirms no back-to-back burst of knob log lines with near-zero interval appears (the failure mode TASK-026 was written to bound) — if one is observed, record the timestamps and file a new bug ticket rather than checking this AC
- [ ] #3 TASK-025's AC #3 is checked via backlog task_edit once this ticket's measurement confirms the rate
- [ ] #4 The measured interval (median and any observed spread) is recorded in this ticket's implementation notes as a number, per this project's established convention (see TASK-018.04's jitter measurements) — not just pass/fail
<!-- AC:END -->
