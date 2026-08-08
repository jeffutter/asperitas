---
id: TASK-020
title: Build podtest diagnostic harness binary
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-08 02:26'
updated_date: '2026-08-08 04:15'
labels:
  - planned
dependencies:
  - TASK-018.01
  - TASK-018.02
  - TASK-018.03
priority: high
type: task
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a `firmware/src/bin/podtest.rs` binary that streams all Pod control surface data over USB CDC serial, enabling TASK-018.04 to verify every control on hardware.

**What it must do:**

1. **Knobs** — Create `Knobs` via `unsafe { steal() }` (same pattern as main.rs's `knob_poll_task`). Poll at ~100 Hz and stream `(knob1, knob2)` values as formatted floats over serial. This lets you sweep knobs by ear/eye and record peak-to-peak jitter magnitude when held still.

2. **Encoder + buttons** — Create `ControlSurface` from the five Pod pins. Call `poll()` in the same loop and stream each `ControlEvent` variant with a timestamp or counter so you can confirm encoder direction, one-detent-per-click ratio, and button single-fire behaviour.

3. **LED 2** — Create `Led2` and cycle through its colours (Off, Red, Green, Blue, Yellow, Cyan, Magenta, White) with ~1s per step so you can visually verify active-low polarity. LED 1 must be untouched — `asperitas_logging::led::init()` should still own D20/D19/D18 for boot/panic stages.

4. **Serial output format** — Use `info!` from `asperitas_logging` (already routed to USB CDC via `log-usb` feature). Keep lines short and parseable. Example:
   ```
   [podtest] k1=0.342 k2=0.781
   [podtest] ENC +1
   [podtest] BTN1 press
   [podtest] led2=cyan
   ```

**Dependencies:** Depends on TASK-018.01 (pins), TASK-018.02 (knobs), TASK-018.03 (encoder/buttons). All three are Done.

**Why not main.rs?** main.rs is designed for audio passthrough with DSP processing. It has no serial output of control state (the defmt logger is no-op, and even if it weren't, nothing logs knob positions). A dedicated binary keeps diagnostic code separate from production firmware.

**Build verification:** Must compile for thumbv7em-none-eabihf alongside existing binaries. Flash with `make flash-all BINARY=podtest`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 firmware/src/bin/podtest.rs exists and compiles for thumbv7em-none-eabihf
- [x] #2 Knob values stream over USB CDC serial at ~100 Hz as normalised floats
- [x] #3 Encoder delta events logged with signed direction
- [x] #4 Button press/release events logged distinctly
- [x] #5 LED 2 cycles through colours with visual delay (~1s per step)
- [x] #6 LED 1 remains owned by asperitas_logging for boot/panic stages
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Implementation Plan: podtest diagnostic harness binary

## File created
- firmware/src/bin/podtest.rs — new binary, discovered automatically by Cargo

## Structure (mirrors blinky.rs)

### Boilerplate
- #![no_std], #![no_main]
- Re-export asperitas_logging::panic_handler as global panic handler
- #[defmt::panic_handler] — infinite loop with NOP (same as blinky/main)
- #[defmt::global_logger] struct Logger — no-op defmt logger (required by embassy-stm32)
- bind_interrupts!(pub struct UsbIrqs { OTG_FS => usb::InterruptHandler })

### Main entry point
#[embassy_executor::main] async fn main(_spawner: embassy_executor::Spawner)

### Boot sequence
1. info!("podtest booting")
2. let config = daisy_embassy::default_rcc()
3. let p = daisy_embassy::hal::init(config)
4. let board: DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p)
5. Discard board.usb_peripherals (USB init steals them)
6. asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18) — LED 1 ownership
7. let _usb_handle = asperitas_logging::usb::init(UsbIrqs)
8. asperitas_logging::led::set_global_state(LedState::Running)

### Hardware construction
- Knobs: Knobs::new(adc1, knob1_pin, knob2_pin) — use unsafe { steal() } for ADC1 + pins PC4/PC0 matching main.rs pattern
- Encoder/buttons: ControlSurface::new(enc_a, enc_b, click, button1, button2) — use unsafe { steal() } for PD11, PA0, PB6, PG9, PA2
- LED 2: Led2::new(red_pin, green_pin, blue_pin) — use unsafe { steal() } for PB1, PA1, PA4

Pin ownership: DaisyBoard does not expose Pod pins (daisy-embassy only exposes Seed pins). Use unsafe { steal() } for each pin, same as main.rs's knob_poll_task. Safe on single-core Cortex-M with exclusive access.

### Main loop (embedded in main, not a separate task)
The main future runs a single polling loop at ~100 Hz:

- Poll knobs: knobs.read() returns (f32, f32), log as "[podtest] k1={:.3} k2={:.3}"
- Poll encoder/buttons: controls.poll(|event| match event to log ENC delta, CLICK press/release, BTN1/BTN2 press/release)
- Cycle LED 2 every 100 ticks (~1 second): iterate [Off, Red, Green, Blue, Yellow, Cyan, Magenta, White], log "[podtest] led2={:?}"
- Tick counter tracks iterations; Timer::after_millis(10).await between iterations

### Concurrent futures
Run USB drain alongside the main polling loop via embassy_futures::select::select(poll_fut, usb_fut). The poll future is an async block containing the infinite loop.

## Verification
1. cargo build --target thumbv7em-none-eabihf --bin podtest must compile
2. Flash: make flash-all BINARY=podtest
3. Observe serial output matches expected format
4. Verify LED 2 cycles through all colours
5. Verify LED 1 still shows green (Running state) — owned by asperitas_logging
<!-- SECTION:PLAN:END -->
