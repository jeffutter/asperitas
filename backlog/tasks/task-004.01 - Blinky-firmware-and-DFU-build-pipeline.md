---
id: TASK-004.01
title: Blinky firmware and DFU build pipeline
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:56'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-004
type: feature
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Write the blinky application and the objcopy/dfu-util invocation that turns it into a flashable image. Based on daisy-embassy's `examples/blinky.rs`.

Keep it in `firmware/src/bin/` permanently as a known-good diagnostic — when something later goes wrong, being able to flash a thing that definitely works is worth a lot.

Build: `cargo objcopy --release --features seed3 -- -O binary firmware.bin`
Flash: `dfu-util -a 0 -s 0x08000000:leave -D firmware.bin`

`0x08000000` is the STM32H750's 128 KB internal flash via the built-in system bootloader. Sufficient for now; larger applications later need the Daisy bootloader relocating into QSPI.

The agent cannot verify this works — no board access. Produce the artifact and the documented command sequence; TASK-004.02 confirms it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `cargo objcopy` produces a valid raw .bin from the firmware workspace
- [ ] #2 Blinky source retained in firmware/src/bin/ as a permanent diagnostic
- [ ] #3 The full build-and-flash command sequence is documented
<!-- AC:END -->
