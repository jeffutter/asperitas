---
id: TASK-018.01
title: Create asperitas-pod crate with the Pod pin map and LED 2 driver
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-07 23:26'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Implementation Plan for asperitas-pod crate
================================================

### Approach

Create `crates/asperitas-pod` following the `asperitas-logging` pattern exactly:
- Optional embassy deps behind a single feature (`pod-hw`)
- Host-compiles with no features; target-compiles with feature enabled
- Pin map as `Peri<derived-lifetime, PXn>` type aliases + struct
- LED 2 driver with simple GPIO on/off (no PWM)

### Verified Pin Mapping (from daisy-embassy pins_seed.rs)

| Pod function | Seed D# | STM32H750 pin | Peripheral |
|---|---|---|---|
| Button 1 (SW_1) | D27 | PG9 | SAI2 SD FS |
| Button 2 (SW_2) | D28 | PA2 | SAI2 SCK, ADC11 |
| Encoder A | D26 | PD11 | SAI2 SD A |
| Encoder B | D25 | PA0 | SAI2 SD B, ADC10 |
| Encoder click | D13 | PB6 | USART1 Tx, I2C4 SCL |
| LED 2 red | D17 | PB1 | ADC2 |
| LED 2 green | D24 | PA1 | SAI2 MCLK, ADC9 |
| LED 2 blue | D23 | PA4 | DAC OUT 1, ADC8 |
| Knob 1 | D21 | PC4 | ADC6 |
| Knob 2 | D15 | PC0 | ADC0 |

LED 1 pins (owned by asperitas-logging — must NOT appear here):
- D20=PC1, D19=PA6, D18=PA7

Zero overlap between LED 1 and LED 2 pin sets — no Peri exclusivity conflict possible.

### Files to create

**1. crates/asperitas-pod/Cargo.toml**
Mirror asperitas-logging structure:

**2. crates/asperitas-pod/src/lib.rs**
- `#![no_std]`
- Feature-gated modules for pod-hw
- Default no-op init when no features

**3. crates/asperitas-pod/src/pins.rs** (feature-gated)
Pin map struct mirroring daisy-embassy's DaisyPins shape:

```rust
pub struct PodPins<'a> {
    pub sw_1:   Peri<'a, hal::peripherals::PG9>,   // D27 - Button 1
    pub sw_2:   Peri<'a, hal::peripherals::PA2>,   // D28 - Button 2
    pub enc_a:  Peri<'a, hal::peripherals::PD11>,  // D26 - Encoder A
    pub enc_b:  Peri<'a, hal::peripherals::PA0>,   // D25 - Encoder B
    pub enc_sw: Peri<'a, hal::peripherals::PB6>,   // D13 - Encoder click
    pub led2_r: Peri<'a, hal::peripherals::PB1>,   // D17 - LED 2 red
    pub led2_g: Peri<'a, hal::peripherals::PA1>,   // D24 - LED 2 green
    pub led2_b: Peri<'a, hal::peripherals::PA4>,   // D23 - LED 2 blue
    pub knob1:  Peri<'a, hal::peripherals::PC4>,   // D21 - Knob 1
    pub knob2:  Peri<'a, hal::peripherals::PC0>,   // D15 - Knob 2
}
```

Each field also gets a type alias (e.g. `pub type PodSw1<'a> = Peri<'a, hal::peripherals::PG9>;`) matching daisy-embassy naming convention.

**4. crates/asperitas-pod/src/led.rs** (feature-gated)
Minimal LED 2 driver — simple GPIO on/off, no singleton:

```rust
/// LED polarity — active-low (verified per docs/reference/daisy-pod.md)
pub const LED_ACTIVE_LOW: bool = true;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Led2Color {
    Off, Red, Green, Blue, Yellow, Cyan, Magenta, White,
}

pub struct Led2 {
    red:   Output<'static>,
    green: Output<'static>,
    blue:  Output<'static>,
}

impl Led2 {
    pub fn new(red: Peri<'_, PB1>, green: Peri<'_, PA1>, blue: Peri<'_, PA4>) -> Self { ... }
    pub fn set_color(&mut self, color: Led2Color) { ... }
    pub fn off(&mut self) { ... }
}
```

Unlike BootLed (which needs StaticCell + atomic state for panic handler access), Led2 is a regular component. Consumer decides storage strategy.

### AC #5 — PWM Decision Documentation

Document in led.rs module comment:
- **PA4 has no TIM alternate function** on STM32H750 (DAC1_OUT1 only, verified via ST CubeMX PeripheralPins.c)
- PA1 (TIM2_CH2/TIM5_CH2/TIM15_CH1N) and PB1 (TIM1_CH3N/TIM3_CH4/TIM8_CH3N) have timer channels, but uniform API requires same drive method for all three channels
- **Decision: on/off only** — seven colours (Off + 7 combos) sufficient for status indication
- Rationale per doc-001 §3: abstractions must justify their cost; hardware PWM adds complexity with marginal benefit for a status LED; software PWM would require a dedicated embassy task with no clear owner yet

### Implementation order

1. Create Cargo.toml with minimal deps
2. Create src/lib.rs with feature gates
3. Create src/pins.rs with full pin map
4. Create src/led.rs with Led2 driver
5. Verify host build (no features) and target build (pod-hw feature)
6. Run cargo test and cargo clippy -D warnings at workspace root

### Verification checklist

- [ ] Host build: `cargo build -p asperitas-pod` (no features)
- [ ] Target build: `cargo build -p asperitas-pod --features pod-hw --target thumbv7em-none-eabihf`
- [ ] No references to PC1, PA6, or PA7 in asperitas-pod source
- [ ] Root `cargo test` passes
- [ ] Root `cargo clippy -D warnings` passes
<!-- SECTION:PLAN:END -->
