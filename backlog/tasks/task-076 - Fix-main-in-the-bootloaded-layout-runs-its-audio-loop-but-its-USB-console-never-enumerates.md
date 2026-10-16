---
id: TASK-076
title: >-
  Fix: main in the bootloaded layout runs its audio loop but its USB console
  never enumerates
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 03:59'
labels:
  - bug
dependencies: []
references:
  - firmware/src/bin/main.rs
  - firmware/memory-bootloaded.x
  - docs/reference/daisy-seed3.md
priority: high
type: bug
ordinal: 178800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found 2026-10-15 while setting up TASK-019.02. `make build BINARY=main LAYOUT=bootloaded FEATURES=seed3` (107,813 B, reset vector in AXI SRAM) was flashed to 0x90040000 with dfu-util -w after a probe reset. The bootloader enumerates ("Daisy Bootloader" on the Mac), hands off, and then the Mac never lists an "Asperitas Debug Console" or any usbmodem node for 60 s across two resets, so CTL SET (host knob override) cannot be sent to main. `rig` in the same layout, same flashing flow, enumerates every time (many runs today), and `main` enumerated in the internal layout (TASK-030.04).

Read over the probe while main was in that state: the core is NOT faulted (CFSR 0, HFSR 0, VTOR 0x24000000); asperitas_logging::led::LED_STATE (0x20003850) reads 1 = Running, so boot passed `prepare_interface` and `start_interface` and reached the audio loop; RCC_CR 0x3f03f025 (PLLs ready); PWR_CR3 0x05000042 (USB33DEN and USB33RDY set); OTG_FS GCCFG 0x00010000 (powered), GUSBCFG 0x40000040 (forced device mode), DCTL 0 (not soft-disconnected), DSTS 0x0040be07 (ENUMSPD full speed, SUSPSTS=1: the bus saw a reset and then went suspended). So the USB peripheral is up and attached but the host never completes enumeration. The audio loop is also running: KNOB_STATE at 0x20002340 reads [0.9999, 0.9999] (both knobs fully clockwise).

Candidate differences from rig, to test not assume: main polls `usb::run()` inside the nested select that also holds the audio callback loop and the LED task, on one cooperative executor, only AFTER `prepare_interface`/`start_interface` await; rig runs audio on its own InterruptExecutor and keeps USB on thread mode. The bootloader teardown (HAL_PWREx_DisableUSBVoltageDetector, then a system reset) is common to both. TASK-038.04.07.04 only ever ran rig in this layout; main was built and size-gated but never booted on hardware in it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause found with a measurement, not a guess: what stops the host completing enumeration for main in the bootloaded layout while rig enumerates, written into the ticket notes with the evidence (registers, symbols or a host-side reproduction)
- [ ] #2 Either main enumerates its console in the bootloaded layout after the fix (verified by `asperitas-rig query` against a bootloaded main on the board, which the owner or the agent may run with the probe-reset and dfu-util -w flow), or the ticket records, with the measurement, why it cannot and where main is supposed to run instead
- [ ] #3 A guard keeps it from coming back: a gate, host test or boot-time check that fails when a bootloaded main would not bring its console up (for example a build-time or CI assertion on whatever the root cause was), with scripts/gates.sh push green
- [ ] #4 docs/reference/daisy-seed3.md records the finding next to the bootloaded-layout section, including which binaries have been run on a board in each layout
<!-- AC:END -->

2026-10-16 (TASK-019.02 session): after 9d01511 moved main's audio callback to an interrupt executor, `/dev/cu.usbmodem101` enumerated on bootloaded main, and survived a power cycle. Not yet root-caused or verified with `asperitas-rig query`, and no guard exists, so criteria are still open.
