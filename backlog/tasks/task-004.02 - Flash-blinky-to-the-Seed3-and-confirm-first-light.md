---
id: TASK-004.02
title: Flash blinky to the Seed3 and confirm first light
status: Done
assignee:
  - '@human'
created_date: '2026-08-01 05:56'
updated_date: '2026-08-01 23:40'
labels: []
dependencies:
  - TASK-004.01
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-004
type: feature
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the physical board. This is the moment that proves the board, the USB-C DFU path, and the cross-compiled binary all work — before audio is involved.

Hold `BOOT`, tap `RESET`, release `BOOT`; the board should enumerate as an STM32 DFU device. Then flash and observe.

**If this fails, the board itself is suspect.** Blinky in C++ via libDaisy is a valid isolation step here — libDaisy has no Seed3 *codec* support, but blinky touches no codec. This is the only place in the project where the C++ fallback is available.

Correct docs/reference/daisy-seed3.md if the real procedure differs from what is written there.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: an LED on the Seed3 blinks under firmware built from this repo
- [x] #2 HUMAN: the documented command sequence works from a clean checkout
- [x] #3 docs/reference/daisy-seed3.md flashing section reflects the actually-working procedure
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
Closed 2026-08-01 on the owner's own hardware observation, not on a build succeeding.

**AC #1** — `make flash-all BINARY=blinky` produced a visibly blinking Pod RGB LED 1
(D20/D19/D18), then steady green once USB was up. Note the LED is on the *Pod*, not the
Seed's onboard user LED; blinky never touches PC7, contrary to what the docs used to
claim. The `ledtest` bisect binary independently confirmed the core was running.

**AC #2** — the sequence in the README works, but it did not work as written when this
ticket was opened. Getting there required four fixes, all landed:

1. `memory.x` declared RAM `LENGTH = 1M`. AXI SRAM is 512 KB; `cortex-m-rt` derives the
   initial SP from `ORIGIN + LENGTH`, so every binary hard-faulted on its first stack
   push, before `main`. This was the actual cause of the dead board — DFU was never the
   problem.
2. `build.rs` never emitted `cargo:rustc-link-search`, so its `OUT_DIR` copy of
   `memory.x` was inert.
3. `cortex_m::asm::bkpt()` in the panic paths escalated to HardFault with no probe
   attached, discarding the LED and serial diagnostics a panic was supposed to produce.
4. `make flash` trusted dfu-util's exit code, which is 74 even on a fully successful
   `:leave`.

**AC #3** — `docs/reference/daisy-seed3.md` updated: `:leave` **works** on Seed3 (the
previous "unreliable on STM32H7" note was wrong and actively misdirected this
investigation); exit-74-on-success is explained; DFU mode is documented as non-sticky;
the blinky LED description is corrected; the stale `memory.x` FLASH-length note is
replaced with the RAM note that actually matters.
<!-- SECTION:NOTES:END -->
