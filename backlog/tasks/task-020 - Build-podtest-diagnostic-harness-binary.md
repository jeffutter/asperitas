---
id: TASK-020
title: Build podtest diagnostic harness binary
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-08 02:26'
updated_date: '2026-08-08 04:00'
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
