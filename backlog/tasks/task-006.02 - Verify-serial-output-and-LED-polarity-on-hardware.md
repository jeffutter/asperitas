---
id: TASK-006.02
title: Verify serial output and LED polarity on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-05 16:45'
labels: []
dependencies:
  - TASK-006.01
  - TASK-013
  - TASK-006.03
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-006
type: feature
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board. Confirm the probe-free debug channel actually works — this is the tooling everything else will be debugged through for the next several weeks, so it needs to be trustworthy before it is relied on.

LED drive polarity cannot be determined without looking at the board; flip the constant from TASK-006.01 if the LEDs read inverted, and record the answer in docs/reference/daisy-pod.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: running firmware prints text visible on the host over USB CDC serial
- [x] #2 HUMAN: LED boot stages are visually distinguishable
- [ ] #3 HUMAN: a deliberate panic reaches the developer via LED state and via serial
- [x] #4 LED drive polarity verified and documented in docs/reference/daisy-pod.md
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
**Partially verified 2026-08-01 — stays To Do.** Three of four criteria were *not*
demonstrated during the bring-up session, so this ticket is not closed.

**AC #4 — done.** Polarity is **active-low**: driving a channel pin `Low` lights it.
Established with `firmware/src/bin/ledtest.rs`, which drives D20/D19/D18 directly and
bypasses `LED_ACTIVE_LOW` entirely, so it is an independent measurement rather than a
confirmation of the existing guess. Observed red/green/blue/black/white maps `Low` → lit
in every step; active-high would have produced the visually distinct complement
(cyan/magenta/yellow/white/black). Recorded in `docs/reference/daisy-pod.md` with the
full mapping table, and `led.rs`'s comment no longer says this ticket is pending.
`LED_ACTIVE_LOW = true` was already correct and did not change.

**AC #1 — not met.** A blocking bug was found and fixed: `usb::run()` awaited
`cdc.wait_connection()` *before* `usb_dev.run()` was ever polled, but `usb_dev.run()` is
what drives enumeration, so neither future could ever progress. With that fixed the board
now enumerates — `ioreg` shows "Asperitas Debug Console" and a `/dev/cu.usbmodem*` node
appears. **But enumeration is not output.** Nobody attached a terminal and read a log
line. Needs: `screen /dev/cu.usbmodem<N> 115200` and confirmation that "Booting..." and
"USB logging initialized" actually arrive.

**AC #2 — not met.** Only the `Running` state (steady green) was observed. Boot is fast
enough that the pre-init red blink went by unseen, so "distinguishable" is untested.
Consider inserting a temporary delay before `set_global_state(Running)` to make the
pre-init stage observable.

**AC #3 — not met.** No panic was induced. Needs a deliberate `panic!()` build, with both
the solid-red LED *and* the panic message over serial confirmed. Note this path was also
broken until this session: `cortex_m::asm::bkpt()` in the panic handler escalated to a
HardFault with no probe attached, so the LED write and pipe write executed but were never
observable. That fix is landed but unverified on hardware.
<!-- SECTION:NOTES:END -->
