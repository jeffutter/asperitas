---
id: TASK-006.01
title: Implement USB CDC serial logging facade and LED boot stages
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 18:36'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-006
type: feature
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two layers, because they fail differently:

1. **USB CDC-ACM over the Seed3's onboard USB-C** — the real channel: text logging from running firmware, no extra hardware. daisy-embassy ships `examples/usb_serial.rs` as a starting point.
2. **Pod RGB LEDs as boot-stage indicator** — for when USB itself hasn't come up, which is exactly when layer 1 can tell you nothing. Distinguishable states for pre-init / clocks up / audio running / panicked.

Put logging behind a thin facade with feature-selected backends (`log-usb` / `log-defmt`) rather than sprinkling either API through the codebase. When a probe arrives, adding defmt must be a feature flag, not a refactor.

Panics must reach the user: a panic handler that lights a distinctive LED state and, if USB is up, prints the message.

Note libDaisy's LED driver inverts polarity — write it so polarity is a single constant to flip, since TASK-006.02 will determine the truth. Pin map in docs/reference/daisy-pod.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Logging goes through a facade with feature-selected backends; adding a defmt backend needs no call-site changes
- [ ] #2 USB CDC serial logging implemented and compiles
- [ ] #3 LED boot-stage indicator covers at least pre-init, running, and panicked
- [ ] #4 Panic handler sets a distinctive LED state and prints over serial when USB is up
<!-- AC:END -->
