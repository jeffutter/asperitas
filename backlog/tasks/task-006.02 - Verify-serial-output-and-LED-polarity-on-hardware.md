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
**Partially verified — stays To Do.** AC #2 and #4 are met; #1 and #3 still need a terminal attached.

**AC #4 — done 2026-08-01.** Polarity is **active-low**: driving a channel pin `Low` lights it.
Established with `firmware/src/bin/ledtest.rs`, which drives D20/D19/D18 directly and
bypasses `LED_ACTIVE_LOW` entirely, so it is an independent measurement rather than a
confirmation of the existing guess. Observed red/green/blue/black/white maps `Low` -> lit
in every step; active-high would have produced the visually distinct complement
(cyan/magenta/yellow/white/black). Recorded in `docs/reference/daisy-pod.md` with the
full mapping table.

**AC #2 — done 2026-08-05.** Confirmed on hardware by the owner: steady red for ~3 s, then
steady green, using `make flash-all FEATURES="seed3 slow-boot"`. The two stages are
plainly distinguishable.

This needed TASK-006.03 first, and the cause was not what this ticket originally guessed.
The note below said 'boot is fast enough that the pre-init red blink went by unseen'; in
fact `led::init` never drove the pins at all, so the LED was dark for the whole boot and
an arbitrarily slow boot would still have shown nothing. TASK-006.03 made `init` drive
PreInit directly, which is what made the stage exist to be seen; `slow-boot` then holds it
long enough to compare against Running.

Also found during this session: `firmware/Makefile` interpolated `--features $(FEATURES)`
unquoted, so `FEATURES="seed3 slow-boot"` split into two shell words and cargo rejected
`slow-boot` as an unexpected positional argument. Both features therefore could not be
combined via make at all. Fixed by quoting in the `build` and `check` targets.

**AC #1 — not met.** The board enumerates (`ioreg` shows 'Asperitas Debug Console', a
`/dev/cu.usbmodem*` node appears), and an earlier blocking bug is fixed: `usb::run()` used
to await `cdc.wait_connection()` before `usb_dev.run()` was ever polled, but
`usb_dev.run()` is what drives enumeration, so neither future could progress.
**Enumeration is not output.** Nobody has yet attached a terminal and read a log line.

Best demonstrated with `make flash-all BINARY=panictest`, whose 10 s countdown emits a
fresh line every second and so cannot be missed. Note that main.rs's own boot lines
(`Booting...`, `USB logging initialized`, `Audio interface ready`) are expected to be
missed: the drain loop unblocks when the host *configures* the interface, about a second
after boot, which is well before a terminal can be started by hand. If those lines are
wanted reliably, gate the drain loop on `cdc.dtr()` — which flips when a terminal actually
opens the port — rather than on `wait_connection()`.

**AC #3 — not met.** No panic has been induced on hardware. Needs `BINARY=panictest` with
a terminal attached, confirming *both* green -> steady red *and* a `PANIC: ... at
src/bin/panictest.rs:L:C` line with no trailing NUL padding.

Two earlier blockers on this path are fixed but unverified on hardware:
`cortex_m::asm::bkpt()` escalated to a HardFault with no probe attached (fixed under
TASK-013/006.01), and the handler wrote the message into LOG_PIPE, which only the halted
executor could ever have drained (fixed under TASK-006.03 via `usb::emit_blocking`).

Weak joint if AC #3 fails while the LED half passes: `emit_blocking` calls
`UsbDevice::run()` a second time while the abandoned future from `usb::run()` still holds
a `&mut` to the same device. Expected to work on a halted single-core board where the
first future is never polled again, but it is the least certain step in that path.
<!-- SECTION:NOTES:END -->
