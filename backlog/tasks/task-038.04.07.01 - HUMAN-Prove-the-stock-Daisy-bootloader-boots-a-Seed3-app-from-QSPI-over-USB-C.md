---
id: TASK-038.04.07.01
title: 'HUMAN: Prove the stock Daisy bootloader boots a Seed3 app from QSPI over USB-C'
status: Done
assignee:
  - '@human'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-15 15:52'
labels:
  - task
dependencies: []
parent_task_id: TASK-038.04.07
priority: high
ordinal: 140800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Unproven: libDaisy has no Seed3 support and the Seed3 USB-C differs from earlier Seeds. On the board, establish (1) the stock DaisyBootloader (grace period / DFU into QSPI at 0x90040000) can be installed on the Seed3 and enumerates over USB-C, (2) it drives this QSPI part, (3) the existing blinky (or ledtest) linked for the bootloaded layout (QSPI/AXI SRAM, below the 0x100000 excerpt slot start from TASK-038.04.01) runs, (4) audio passthrough or main still brings up SAI. Record exact bootloader version, flash commands, and the app region/link addresses that worked in docs/reference/daisy-seed3.md. If the bootloader cannot work on Seed3, record that and stop: the parent must then be re-decided with the owner (opt-level/split-binary routes, measured sizes in the parent description). Do not mark Done unattended.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: Stock Daisy bootloader installs on Seed3 and enumerates over USB-C
- [x] #2 HUMAN: A trivial Rust app linked for the bootloaded layout is flashed to QSPI and runs (LED/serial evidence)
- [x] #3 HUMAN: Working addresses, commands and failure notes recorded in docs/reference/daisy-seed3.md
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-15: closed in a live session with the owner at the bench; the owner agreed each criterion was met. Stock DaisyBootloader v6.4.0 (intdfu-2000ms, 130,188 bytes) installed via system DFU and enumerated as 'Daisy Bootloader' with the QSPI flash as its DFU target (0x90000000, app region 0x90040000). ledtest linked at FLASH=0x24000000 (480K), RAM=DTCM 0x20000000 (128K), reset vector 0x24000299, was flashed to 0x90040000 and the Pod RGB LED cycled as from internal flash. Re-entry: after a reset the DFU interface is visible from ~0.7 s for the 2 s grace period; probe-rs reset then 'dfu-util -w' reflashed with no button press. All recorded in docs/reference/daisy-seed3.md (commit 9a3dc89). NOT proven here: description step 4 (main or rig bringing up SAI under the bootloaded layout) - needs the SAI DMA buffers in D2, which is TASK-038.04.07.02, and TASK-038.04.07.04 re-measures rig callback timing. Board state at close: bootloader plus ledtest in QSPI; main and rig are not in internal flash (restore rig with make probe-flash BINARY=rig, which overwrites the bootloader).
<!-- SECTION:NOTES:END -->
