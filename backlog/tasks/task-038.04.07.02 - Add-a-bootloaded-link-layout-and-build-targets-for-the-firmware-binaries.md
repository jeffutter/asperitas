---
id: TASK-038.04.07.02
title: Add a bootloaded link layout and build targets for the firmware binaries
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-15 16:19'
labels:
  - planned
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
- [x] #1 Bootloaded layout builds for main, rig (all three feature sets) and blinky; rig with 762392c applied links with at least 2 KB spare in its target region, sizes recorded
- [x] #2 Excerpt slots at QSPI 0x100000 do not overlap the app region (asserted by a gate or linker ASSERT)
- [x] #3 Elf-provenance / load-address gates updated and all firmware gates tiers pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 05d78b5. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed: firmware/memory-bootloaded.x (FLASH 0x24000000 480K, RAM DTCM 128K, RAM_D2 288K for .sram1_bss), selected by ASPERITAS_LAYOUT in build.rs and stamped as layout= in .asp.prov. IMPORTANT: the internal layout file was renamed memory.x -> memory-internal.x. Measured: lld resolves 'INCLUDE memory.x' from the cwd before any -L path, so a crate-root memory.x silently beat the OUT_DIR copy (an ELF stamped bootloaded linked for internal flash). Makefile: LAYOUT=internal|bootloaded, images <bin>-bootloaded.bin flashed to 0x90040000, elf-check compares layout, probe-flash/probe-run refuse bootloaded. Gates: check-image-load-addresses.sh picks the layout from the ELF's lowest file-backed LMA and adds R4 (image and region end below EXCERPT_AREA_START, both parsed from memory-bootloaded.x QSPI_APP_OFFSET marker and excerpt.rs), R5 (.sram1_bss inside RAM_D2), R6 (>= 2048 spare); new push gate bootloaded-builds (scripts/check-bootloaded-builds.sh) builds and grades blinky, main, rig x3 before the cross-build pair; ledger refreshed with GATE_COSTS_BOOTSTRAP=1 (new gate unpriced). check-doc-artifact-names derives legal image names for both layouts.
Sizes (-O binary, bootloaded, bytes; region 491,520): blinky 78,438; main 107,621; rig 125,762 / stim-ess 127,586 / stim-pulse 125,986. AC1 with 762392c: cherry-picked onto HEAD in a scratch worktree (conflicts in rig.rs/build.rs resolved by hand, worktree removed): rig 144,137 / 145,929 / 144,489 bytes, about 347 KB spare; the same tree overflows internal flash by 12.5-14.8 KB. Reset vector 0x24000299, SP 0x20020000, .sram1_bss at 0x30000000 verified in the ELF. Not verified (no hardware): that the image boots and audio runs from this layout; that is TASK-038.04.07.04. Gates: scripts/gates.sh push green (29 gates).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the bootloaded link layout (memory-bootloaded.x), renamed the internal one to memory-internal.x so build.rs's selection actually wins, wired LAYOUT through the Makefile, build.rs, provenance and elf-check, extended the load-address gate with QSPI/excerpt, D2 and spare-space rules, and added a push gate that builds and grades main, blinky and rig (3 feature sets) bootloaded. rig with the install path (762392c) links with about 347 KB spare. Push tier green; booting it is unverified and belongs to TASK-038.04.07.04.
<!-- SECTION:FINAL_SUMMARY:END -->
