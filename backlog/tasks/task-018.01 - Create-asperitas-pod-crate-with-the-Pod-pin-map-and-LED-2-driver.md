---
id: TASK-018.01
title: Create asperitas-pod crate with the Pod pin map and LED 2 driver
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-07 23:15'
labels:
  - planned
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create `crates/asperitas-pod`, the crate that holds the Daisy Pod pin map and the control drivers built on it. This subtask delivers the crate, the pin map, and the simplest driver (LED 2); knobs are TASK-018.02 and encoder/buttons are TASK-018.03.

Follow the `asperitas-logging` pattern: the crate lives in the root host workspace (`crates/*` is the members glob) with every embassy dependency `optional = true` behind a feature. That is what keeps root `cargo test` and `cargo clippy` working without a thumbv7em toolchain. Consequence to accept knowingly: with no features enabled the crate compiles to almost nothing on the host. That is the same trade asperitas-logging already makes.

Express the pin map as `Peri<derived-lifetime, PXn>` type aliases plus a struct, mirroring daisy-embassy `src/pins/pins_seed.rs` in shape and naming. Two reasons: it reads like the code it sits beside, and it can be offered upstream close to unchanged (docs/reference/daisy-pod.md notes the Pod map is a good upstream candidate).

LED OWNERSHIP BOUNDARY — the part that will bite if ignored. `asperitas-logging::led` already owns Pod LED 1 (D20/D19/D18 = PC1/PA6/PA7) as a StaticCell singleton used for boot-stage and panic indication, with those pin types hardcoded in its `init` signature. asperitas-pod must take LED 2 (D17/D24/D23 = PB1/PA1/PA4) and must not touch LED 1. Two owners of the same physical pins is exactly the `Peri` exclusivity violation that TASK-016 and TASK-017 existed to remove; do not reintroduce it from the other side.

Colour mixing on LED 2 needs a decision that rests on a hardware fact to establish first: hardware PWM needs a timer channel per pin, and PA4 (LED 2 blue) is DAC_OUT1 and may have no TIM alternate function on the STM32H750. Check all three channels. If any lacks a timer, hardware PWM for LED 2 is off the table — use software PWM in an embassy task, or on/off only, and record which and why. On/off gives seven colours, which may well be enough for a status indicator. Do not add PWM speculatively; doc-001 section 3 is explicit that abstractions must justify their cost.

Peripheral availability note that saves a wrong turn: `new_daisy_board!(p)` only partially moves `p`, so unclaimed peripherals — `p.ADC1` and every Pod control pin — are still available after the macro runs. The board struct does not need extending and daisy-embassy does not need forking.

Pin assignments come from the table in docs/reference/daisy-pod.md. Do not re-derive them from libDaisy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crates/asperitas-pod exists as a root-workspace member and builds both for the host with no features and for thumbv7em-none-eabihf with its hardware feature enabled
- [ ] #2 The pin map covers every control in the docs/reference/daisy-pod.md table: both knobs, encoder A/B/click, both buttons, and both RGB LEDs
- [ ] #3 LED 2 exposes an API to set its colour, honouring the Pod active-low drive documented in daisy-pod.md; hardware confirmation is TASK-018.04
- [ ] #4 asperitas-pod contains no reference to PC1, PA6, or PA7 — LED 1 remains solely owned by asperitas-logging
- [ ] #5 The LED 2 drive choice (hardware PWM, software PWM, or on/off) is recorded together with the per-channel timer-availability finding that decided it
- [ ] #6 Root cargo test and cargo clippy with -D warnings pass with the new crate as a workspace member
<!-- AC:END -->
