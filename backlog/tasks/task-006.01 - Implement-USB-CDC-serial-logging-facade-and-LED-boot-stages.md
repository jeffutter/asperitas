---
id: TASK-006.01
title: Implement USB CDC serial logging facade and LED boot stages
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 18:51'
labels:
  - planned
dependencies:
  - TASK-006.01.01
  - TASK-006.01.02
  - TASK-006.01.03
  - TASK-006.01.04
  - TASK-006.01.05
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Plan

### Architecture

New workspace crate `crates/asperitas-logging/` provides a feature-selected logging facade using the `log` crate. Call-sites use `info!`, `debug!`, etc. from the re-exported macros — backend selection is entirely at init time via Cargo features.

### Sub-ticket execution order

1. **TASK-006.01.01** — Create the crate skeleton with facade, trait definitions, and no-op default backend. Establishes the module structure other tickets build into.
2. **TASK-006.01.02** — Implement USB CDC-ACM backend. Fills in `backend_usb.rs` with embassy-usb integration. Depends on 01 for the crate structure.
3. **TASK-006.01.03** — LED boot-stage indicator. Independent of USB work; can run in parallel with 02. Uses GPIO pins from the Pod pin map (D18-D20).
4. **TASK-006.01.04** — Custom panic handler. Replaces panic-halt. Depends on 03 (needs LED panicked state) and touches 02 (USB write during panic).
5. **TASK-006.01.05** — Integration into main.rs and blinky.rs. Ties everything together: spawns USB task alongside audio, wires LED stages, enables log-usb feature. Blocks on all others.

### Key design decisions

- **Separate crate vs common modules:** Chose separate crate (`asperitas-logging`) for clean separation of concerns and future reuse when adding defmt backend or live TUI logging.
- **log crate over tracing:** Lighter swap story for simple backends; `tracing` adds significant binary size overhead not justified for embedded logging.
- **GPIO over PWM for LEDs:** Simple on/off per color channel is sufficient for visually distinct states (red blink, green solid, red strobe). Avoids PWM timer complexity.
- **Polarity constant:** Single `const LED_ACTIVE_LOW: bool` makes TASK-006.02 hardware verification a one-line change.
- **Windows compatibility descriptors:** Copied verbatim from daisy-embassy example (class 0xEF, sub 0x02, proto 0x01, composite_with_iads=true).
- **vbus_detection = false:** Safe default for Pod which has no VBUSEN pin wired.

### Risks

1. **USB enumeration timing:** `wait_connection()` blocks forever if no host attached. The USB task loops on reconnect so this is handled gracefully.
2. **Panic-time USB writes:** Async executor is halted during panic. Using direct PAC register access for synchronous USB writes (~30 lines). Fall back to LED-only if too complex.
3. **Flash budget:** USB descriptors + buffers add ~2-3 KB RAM. Should fit within 128 KB internal flash alongside audio passthrough. Verify binary size in integration ticket.
4. **LED polarity unknown:** Code structured so flipping the constant fixes inversion. TASK-006.02 determines truth.
<!-- SECTION:NOTES:END -->
