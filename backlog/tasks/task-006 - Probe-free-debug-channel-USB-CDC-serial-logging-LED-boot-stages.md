---
id: TASK-006
title: 'Probe-free debug channel: USB CDC serial logging + LED boot stages'
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:46'
updated_date: '2026-08-01 05:56'
labels: []
dependencies:
  - TASK-004
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/daisy-seed3.md
priority: high
type: feature
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Without an ST-Link there is no `defmt`/RTT, which means no panic messages and no diagnostics — a bad position to debug device-only problems from. Build a replacement now, while the firmware is still simple enough that the channel itself is easy to trust.

**Two layers, because they fail differently:**

1. **USB CDC-ACM serial over the Seed3's onboard USB-C.** This is the real one: actual text logging from running firmware, no extra hardware. daisy-embassy ships `examples/usb_serial.rs` as a starting point. Flash over DFU, then the application enumerates as a serial device.

2. **Pod RGB LEDs as a boot-stage indicator**, for the case where USB itself hasn't come up — which is exactly when layer 1 can't tell you anything. A small set of distinguishable states (pre-init / clocks up / audio running / panicked) is enough. Note libDaisy's LED driver inverts polarity; verify drive polarity on first bring-up rather than assuming. Pin map is in docs/reference/daisy-pod.md.

Design this so `probe-rs` + `defmt` slots in later **without rework** — put logging behind a thin facade with feature-selected backends (`log-usb` / `log-defmt`) rather than sprinkling either API through the codebase. When the probe arrives it should be a feature flag, not a refactor.

Panics must reach the user somehow: a panic handler that lights a distinctive LED state and, if USB is up, prints the panic message.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks complete
<!-- AC:END -->
