---
id: TASK-038.04.07.02
title: Add a bootloaded link layout and build targets for the firmware binaries
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-09 02:25'
labels:
  - task
dependencies:
  - TASK-038.04.07.01
parent_task_id: TASK-038.04.07
priority: high
ordinal: 141800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the bench proof (sibling HUMAN ticket) names the working addresses: add a memory.x variant (firmware/memory.x) placing code in the bootloader app region (QSPI XIP or AXI SRAM per the proof) strictly below QSPI offset 0x100000 (excerpt slots, TASK-038.04.01), update .cargo/linker config and the make build targets so each firmware binary can be built for the bootloaded layout, keeping the internal-flash layout available for blinky/ledtest as the fallback. Gate: all firmware builds succeed for seed3, seed3+stim-ess, seed3+stim-pulse with 'rig' carrying the install path from wip/TASK-038.04.05-excerpt-install (762392c) applied in a scratch tree, with sizes recorded in notes; the elf-provenance/load-address gates are updated for the new addresses. Verify with cargo build and the project's gates tiers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bootloaded layout builds for main, rig (all three feature sets) and blinky; rig with 762392c applied links with at least 2 KB spare in its target region, sizes recorded
- [ ] #2 Excerpt slots at QSPI 0x100000 do not overlap the app region (asserted by a gate or linker ASSERT)
- [ ] #3 Elf-provenance / load-address gates updated and all firmware gates tiers pass
<!-- AC:END -->
