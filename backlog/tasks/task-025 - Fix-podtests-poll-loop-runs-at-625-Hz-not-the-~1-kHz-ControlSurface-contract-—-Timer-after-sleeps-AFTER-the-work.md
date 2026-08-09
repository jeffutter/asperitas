---
id: TASK-025
title: >-
  Fix: podtest's poll loop runs at 625 Hz, not the ~1 kHz ControlSurface
  contract — Timer::after sleeps AFTER the work
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-08-09 04:33'
updated_date: '2026-08-09 04:47'
labels:
  - review-followup
dependencies:
  - TASK-021
documentation:
  - docs/reference/daisy-pod.md
priority: medium
type: bug
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-021 raised podtest's loop from 100 Hz to a nominal 1 kHz by setting POLL_INTERVAL_MS = 1. Hardware measurement 2026-08-08 shows the loop actually runs at ~625 Hz — still 1.6x under the documented contract.

MEASUREMENT: median interval between knob log lines is 16 ms; with KNOB_LOG_THROTTLE = 10 that is 1.60 ms per tick, i.e. 625 Hz. Derived from 12320 clean knob samples over 240 s.

ROOT CAUSE: firmware/src/bin/podtest.rs:225 awaits Timer::after_millis(POLL_INTERVAL_MS) at the END of the loop body. That sleeps a fixed 1 ms IN ADDITION to the work already done, so the period is (work + 1 ms), not 1 ms. The work is not negligible here: two blocking ADC reads at Averaging::Samples16 are ~105 us each (~210 us total), plus GPIO reads and — dominantly — USB CDC formatting and writes. Measured overhead is ~0.6 ms.

This is the same class of error TASK-021 fixed, one level down: TASK-021 corrected the interval constant but kept the after-the-work sleep, so the contract is still not met.

FIX: use embassy_time::Ticker (Ticker::every(Duration::from_millis(1)) + ticker.next().await), which schedules against a fixed period rather than accumulating drift from the work duration. Ticker also degrades sanely if a single iteration overruns.

Apply the same review to firmware/src/main.rs's control-surface task, which was written to the same pattern and is the one that actually matters for shipped behaviour — podtest is only the harness.

SEVERITY CONTEXT — this is a contract violation to fix on principle, not a confirmed source of lost input. The same capture found no evidence of dropped detents at 625 Hz: brisk spins produced +86 and -90 against ~21-22 actual detents, i.e. counts came out HIGH, not low. Button debounce (DEBOUNCE_TICKS = 5) stretches from ~5 ms to ~8 ms, which is still well inside the 1-10 ms bounce window and was not observable by hand. Fix it because encoder.rs documents ~1 kHz as a precondition and callers should honour it, not because the sky is falling.

Note the 4:1 detent-ratio bug (TASK-024) is INDEPENDENT of this and is the actual cause of TASK-018.04 AC #3 failing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 podtest's control-surface loop achieves a measured ~1 kHz, using a fixed-period scheduler (embassy_time::Ticker) rather than a sleep appended after the work
- [ ] #2 firmware/src/main.rs's control-surface task is reviewed for the same after-the-work sleep pattern and corrected if present, or the task notes record why it does not apply
- [ ] #3 HUMAN: a fresh podtest capture shows a knob log line interval of ~10 ms (KNOB_LOG_THROTTLE=10 at 1 kHz), confirming the achieved rate rather than the nominal one
- [ ] #4 nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release succeeds
- [ ] #5 nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest -- -D warnings passes
<!-- AC:END -->
