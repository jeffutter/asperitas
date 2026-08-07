---
id: TASK-018.01
title: Create asperitas-pod crate with the Pod pin map and LED 2 driver
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-07 23:23'
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
- LED 2 driver with simple GPIO on/off (no PWM — see AC #5 justification)

### Files to create

**1. crates/asperitas-pod/Cargo.toml**
Mirror asperitas-logging Cargo.toml structure:
- `[features]`: default = [], pod-hw = [dep:embassy-stm32, dep:static_cell]
- Dependencies: embassy-stm32 (optional, stm32h750ib), static_cell (optional)
- Versions match daisy-embassy: embassy-stm32 0.6.0, static_cell 2

**2. crates/asperitas-pod/src/lib.rs**
- `#![no_std]`
- Feature-gated module: `#[cfg(feature = "pod-hw")] pub mod led;`
- Default init stub when no features: `pub fn init() {}` (or just the empty impl)

**3. crates/asperitas-pod/src/pins.rs** (inside pod-hw feature gate)
Pin map struct with `Peri` type aliases for every Pod control. Type aliases named after libDaisy convention, struct holds them all:

```rust
pub struct PodPins {
    pub sw_1:     Peri<'static, hal::peripherals::PB8>,   // D27 - Button 1
    pub sw_2:     Peri<'static, hal::peripherals::PB9>,   // D28 - Button 2
    pub enc_a:    Peri<'static, hal::peripherals::PC11>,  // D26 - Encoder A
    pub enc_b:    Peri<'static, hal::peripherals::PC12>,  // D25 - Encoder B
    pub enc_sw:   Peri<'static, hal::peripherals::PD0>,   // D13 - Encoder click
    pub led2_r:   Peri<'static, hal::peripherals::PB1>,   // D17 - LED 2 red
    pub led2_g:   Peri<'static, hal::peripherals::PA1>,   // D24 - LED 2 green
    pub led2_b:   Peri<'static, hal::peripherals::PA4>,   // D23 - LED 2 blue
    pub knob1:    Peri<'static, hal::peripherals::PB0>,   // D21 - Knob 1
    pub knob2:    Peri<'static, hal::peripherals::PA5>,   // D15 - Knob 2
}
```

Wait — I need to verify the actual STM32H750 pin assignments for each D-number. The daisy-pod doc lists Seed D-numbers (D27, D28, etc.) but not the STM32 PXn equivalents. I need to look up the Seed3 pinout to resolve D→PXn mapping. This is critical — wrong pins = wrong code.

Actually, the research summary gives LED 2 pins explicitly: PB1/PA1/PA4. For the other controls, I need to derive from daisy-embassy's Seed3 pinout or libDaisy's daisy_pod.cpp. Let me check if there's a reference for the full D→PXn mapping.

The safest source is daisy-embassy's `pins_seed.rs` which maps D-numbers to STM32 pins. Since we don't have it locally, I should check the daisy-embassy repo online or derive from known Seed3 pinout documentation.

Alternative: look at how the firmware currently uses these pins. If any existing code references specific PXn for Pod controls, that's ground truth.

**Key verification step before coding:** Resolve every D-number to its STM32 pin. Sources in order:
1. daisy-embassy GitHub (src/pins/pins_seed.rs or equivalent)  
2. libDaisy daisy_pod.cpp (already referenced by daisy-pod.md)
3. Existing firmware code that may already use these pins

### LED 2 Driver (src/led.rs)

Minimal driver mirroring BootLed pattern but simpler (no blink task, no atomic state):

```rust
pub const LED_ACTIVE_LOW: bool = true;

pub enum Led2Color {
    Off,       // all channels off
    Red,       // red only
    Green,     // green only
    Blue,     // blue only
    Yellow,   // red + green
    Cyan,      // green + blue
    Magenta,  // red + blue
    White,     // all three
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

No StaticCell singleton needed — unlike BootLed which is shared between async and panic context, Led2 is a regular component passed by value/mut ref. Consumer decides storage strategy.

### AC #5 — PWM Decision Documentation

Record in a doc comment or dedicated constant block:
- PA4 = DAC1_OUT1 only, no TIM alternate function on STM32H750 (verified via CubeMX PeripheralPins.c)
- PA1 and PB1 have timer channels, but uniform API requires same drive method for all three
- Decision: **on/off only** — seven colours sufficient for status indication
- Rationale per doc-001 §3: abstractions must justify their cost; PWM adds complexity with marginal benefit for a status LED

### Integration / Verification

1. `cargo build` — host build with no features (should compile to near-nothing)
2. `cargo build --target thumbv7em-none-eabihf --features pod-hw` — target build
3. `cargo test` — root workspace tests still pass
4. `cargo clippy -D warnings` — clean across workspace
5. Verify no references to PC1/PA6/PA7 (LED 1 pins) exist in asperitas-pod

### Risks

- **Pin mapping correctness**: Must cross-reference D-numbers against STM32H750 pinout. Wrong pin = silent misbehavior. Mitigation: compare against daisy-embassy's pins_seed.rs and/or libDaisy's daisy_pod.cpp.
- **Feature name**: choose something descriptive (`pod-hw` or `hardware`). Keep consistent with asperitas-logging's `log-usb` naming style.
<!-- SECTION:PLAN:END -->
