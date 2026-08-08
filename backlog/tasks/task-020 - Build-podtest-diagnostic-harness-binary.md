---
id: TASK-020
title: Build podtest diagnostic harness binary
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-08 02:26'
updated_date: '2026-08-08 04:10'
labels: []
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
Implementation Plan: podtest diagnostic harness binary\n\n## File created\n-  — new binary, discovered automatically by Cargo\n\n## Structure (mirrors blinky.rs)\n\n### Boilerplate\n- , \n- Re-export  as global panic handler\n-  — infinite loop with NOP (same as blinky/main)\n-  — no-op defmt logger (required by embassy-stm32)\n- \n\n### Main entry point\n\n\n### Boot sequence\n1. \n2. \n3. \n4. \n5. Discard  (USB init steals them)\n6.  — LED 1 ownership\n7. \n8. \n\n### Hardware construction\n- **Knobs:**  — pass board pins directly (no steal needed; board owns ADC1 and PodPins exposes knob pins)\n  - *Correction:* Check if  has / fields or if we need . If  doesn't expose Pod pins, use  like main.rs does.\n- **Encoder/buttons:**  — same steal-if-needed logic\n- **LED 2:**  — same pattern\n\n### Main loop (embedded in main, not a separate task)\nThe main future runs a single loop that polls everything:\n\n\n\n### Concurrent futures\nRun USB drain alongside the main polling loop:\n\n\n### Pin ownership resolution\nCheck whether  exposes Pod pins. If not (likely — daisy-embassy may only expose Seed pins), use  for each pin, matching main.rs's  pattern. Document why steal is safe (single-core Cortex-M, exclusive access).\n\n## Verification\n1.  — must compile\n2. Flash: \n3. Observe serial output matches expected format\n4. Verify LED 2 cycles through all colours\n5. Verify LED 1 still shows green (Running state) — owned by asperitas_logging
<!-- SECTION:PLAN:END -->
