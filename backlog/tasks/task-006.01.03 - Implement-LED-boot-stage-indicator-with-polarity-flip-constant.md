---
id: TASK-006.01.03
title: Implement LED boot-stage indicator with polarity-flip constant
status: To Do
assignee: []
created_date: '2026-08-01 18:45'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-006.01
priority: high
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement Pod RGB LED boot-stage indicator for pre-init, running, and panicked states.

**What it does:**
- Controls one of the Pod's RGB LEDs (LED 1: D20/D19/D18 = PC1/PA6/PA7) as a status indicator
- States:\n  - **Pre-init:** slow blink red (~1 Hz) — firmware is starting\n  - **Running:** steady green — audio pipeline active\n  - **Panicked:** fast strobe red (~5 Hz) — unrecoverable error\n- Polarity is controlled by a single constant (`const LED_ACTIVE_LOW: bool`) so TASK-006.02 can flip it without code changes

**Implementation plan:**
1. In `asperitas-logging/src/led.rs`:
   - Define `LedState` enum: `PreInit`, `Running`, `Panicked`\n   - Define `BootLed` struct holding GPIO outputs for R/G/B pins\n   - Use simple GPIO output (not PWM) — on/off per color channel is sufficient for distinguishable states\n   - Implement `set_state(&mut self, state: LedState)` with polarity inversion via constant\n   - Implement `blink_task(mut self, state: LedState)` async function that loops on blinking states\n2. Pin mapping from daisy-pod.md:\n   - LED 1 red: D20 = PC1\n   - LED 1 green: D19 = PA6  \n   - LED 1 blue: D18 = PA7\n3. For `PreInit` and `Panicked` states, provide an async blink loop using `embassy_time::Timer`\n4. For `Running` state, set solid green and return (no async loop needed)\n5. Provide `pub fn take_boot_led(p: &Peripherals) -> BootLed` constructor that consumes the raw pins\n6. Note: these pins come from `board.pins.d17..d20` in DaisyPins — need to configure them as GPIO outputs after board init

**Design notes:**
- Using LED 1 (not the onboard user LED on PC7) since it's more visible on the Pod\n- Simple GPIO on/off is preferred over PWM for boot indicators — less complexity, still visually distinct\n- Polarity constant at module level makes TASK-006.02 verification trivial
<!-- SECTION:DESCRIPTION:END -->
