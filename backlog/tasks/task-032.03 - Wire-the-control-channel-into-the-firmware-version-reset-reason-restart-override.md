---
id: TASK-032.03
title: >-
  Wire the control channel into the firmware: version, reset reason, restart,
  override
status: To Do
assignee: []
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 05:59'
labels:
  - task
dependencies:
  - TASK-032.01
  - TASK-032.02
parent_task_id: TASK-032
priority: high
ordinal: 157800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Firmware wiring (non-host-testable part): a task in the firmware binaries that read inbound records, dispatch them using the TASK-032 protocol module, apply overrides to the knob-boundary layer, and emit replies on the framed console. Provide firmware version (from Cargo package version plus git describe if available), reset reason of the current boot read from the STM32H7 RCC reset status register captured before it is cleared, and RESTART via a software system reset. Restart must never make a faulted board harder to recover: it must be serviced without depending on the audio path or channel backpressure, must not mask BOOT/RESET/DFU entry, and the inbound path must not block the panic handler. Document the chosen design in docs/reference/daisy-seed3.md. Must build for thumbv7em targets via the firmware Makefile; compiling is not hardware evidence (TASK-033.01 verifies on the board).
<!-- SECTION:DESCRIPTION:END -->
