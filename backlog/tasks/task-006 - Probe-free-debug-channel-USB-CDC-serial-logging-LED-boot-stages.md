---
id: TASK-006
title: 'Probe-free debug channel: USB CDC serial logging + LED boot stages'
status: Done
assignee:
  - '@human'
created_date: '2026-08-01 05:46'
updated_date: '2026-08-05 17:11'
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
- [x] #1 All subtasks complete
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
All eight descendants Done. The probe-free debug channel is verified end to end on hardware 2026-08-05 and is now trustworthy to debug through:

- Boot stages via LED — steady red then steady green, distinguishable with the slow-boot feature
- Log text over USB CDC — countdown lines arriving in order through the pipe
- Panic via LED — green to steady red
- Panic via serial — 'PANIC: ... at src/bin/panictest.rs:125:9', no NUL padding
- LED polarity — active-low, independently measured and documented in docs/reference/daisy-pod.md

Getting here took two fix tickets beyond the original implementation, both because 'it compiles' had been mistaken for 'it works': TASK-013 (BootLed's states were never wired into firmware) and TASK-006.03 (the panic message could not physically reach the host, the pre-init LED never drove its pins, oversize CDC writes silently dropped logs, and panic messages carried NUL padding). See TASK-006.02's notes for the verification transcript and TASK-006.03's for the defect analysis.

Reusable fixtures left behind: firmware/src/bin/ledtest.rs (is the core running at all), firmware/src/bin/panictest.rs (does a panic reach me), and the slow-boot feature (hold pre-init long enough to see). All three are documented in the README's debugging section.

Known limitation, deliberately not chased: main.rs's own boot lines are expected to be missed, because the drain loop unblocks when the host configures the interface — about a second after boot, well before a terminal can be started by hand. Gate the drain loop on cdc.dtr() instead of wait_connection() if those specific lines are ever wanted reliably.
<!-- SECTION:NOTES:END -->
