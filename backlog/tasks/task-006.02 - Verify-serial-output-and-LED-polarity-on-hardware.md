---
id: TASK-006.02
title: Verify serial output and LED polarity on hardware
status: Done
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-05 17:03'
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
- [x] #1 HUMAN: running firmware prints text visible on the host over USB CDC serial
- [x] #2 HUMAN: LED boot stages are visually distinguishable
- [x] #3 HUMAN: a deliberate panic reaches the developer via LED state and via serial
- [x] #4 LED drive polarity verified and documented in docs/reference/daisy-pod.md
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
**Verified on hardware 2026-08-05. All four criteria met.**

**AC #4 — polarity, 2026-08-01.** Active-low: driving a channel pin `Low` lights it.
Established with `firmware/src/bin/ledtest.rs`, which drives D20/D19/D18 directly and
bypasses `LED_ACTIVE_LOW` entirely, so it is an independent measurement rather than a
confirmation of the existing guess. Observed red/green/blue/black/white maps `Low` -> lit
in every step; active-high would have produced the visually distinct complement
(cyan/magenta/yellow/white/black). Recorded in `docs/reference/daisy-pod.md` with the full
mapping table. `LED_ACTIVE_LOW = true` was already correct and did not change.

**AC #2 — LED boot stages, 2026-08-05.** `make flash-all FEATURES="seed3 slow-boot"`:
steady red for ~3 s, then steady green. Plainly distinguishable.

**AC #1 and #3 — serial output and the panic path, 2026-08-05.** `make flash-all
BINARY=panictest` with `screen` attached produced, verbatim:

    [INFO] panictest: panicking in 10...
    [INFO] USB connected
    [INFO] panictest: panicking in 9...
    ... (8 down to 1)
    PANIC: panictest: deliberate panic, exercising the LED + serial panic path at src/bin/panictest.rs:125:9

LED went green -> steady red at the panic, so both halves of AC #3 are confirmed.

Three details worth keeping:
- `src/bin/panictest.rs:125:9` is exactly where the `panic!` sits, so the location
  plumbing is correct rather than coincidentally plausible.
- The line ends cleanly at `:125:9` with no trailing NUL padding, confirming
  TASK-006.03's defect-4 fix on the wire.
- `[INFO] USB connected` arriving *after* 'panicking in 10...' is correct, not a glitch:
  the first countdown line entered the pipe before the host configured the interface, and
  the drain loop then logged its own message and flushed both in order. Confirms the pipe
  buffers across the pre-enumeration window.

This also retires the uncertainty flagged in TASK-006.03: `emit_blocking` calling
`UsbDevice::run()` a second time, while the abandoned future from `usb::run()` still holds
a `&mut` to the same device, works in practice on the halted single-core board.

**Why three of four criteria had failed before.** Not flakiness — the firmware could not
produce what they asked for, fixed under TASK-006.03:
- The panic message was written into LOG_PIPE, which only the halted executor could ever
  have drained. Now pushed straight to the endpoint by `usb::emit_blocking`.
- `led::init` never drove the pins, so the LED was dark for the entire boot. This ticket
  originally guessed 'boot is fast enough that the pre-init red blink went by unseen'; in
  fact an arbitrarily slow boot would also have shown nothing, so the delay this note
  suggested would not by itself have fixed it.
- `cortex_m::asm::bkpt()` escalated to a HardFault with no probe attached (fixed earlier
  under TASK-013/006.01).
- `firmware/Makefile` interpolated `--features $(FEATURES)` unquoted, so
  `FEATURES="seed3 slow-boot"` split into two shell words and cargo rejected `slow-boot`
  as an unexpected positional argument — the two features could not be combined via make
  at all. Found and fixed during this session.

**Known limitation, deliberately not chased.** main.rs's own boot lines (`Booting...`,
`USB logging initialized`, `Audio interface ready`) are expected to be missed in practice:
the drain loop unblocks when the host *configures* the interface, roughly a second after
boot, well before a terminal can be started by hand. AC #1 is satisfied by panictest's
repeating countdown instead. If those specific lines are ever wanted reliably, gate the
drain loop on `cdc.dtr()` — which flips when a terminal actually opens the port — rather
than on `wait_connection()`.
<!-- SECTION:NOTES:END -->
