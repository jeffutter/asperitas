---
id: TASK-038.04.07.02
title: Add a bootloaded link layout and build targets for the firmware binaries
status: In Progress
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
Planned against b86def6

Approach (single ticket, no sub-tickets: the pieces share one linker contract and ship together).
1. Linker variant. Add firmware/memory-bootloaded.x beside memory.x (same load-bearing comments). Per docs/reference/daisy-seed3.md 'stock Daisy bootloader' section: FLASH = 0x24000000 LENGTH 480K (bootloader copies image from QSPI app region 0x90040000 into AXI SRAM, reset vector in 0x24xxxxxx), RAM = DTCM 0x20000000 128K. .data/.bss/stack go to DTCM. Move the (NOLOAD) .sram1_bss rule to a RAM_D2 region (0x30000000, 288K) because DMA1/2 cannot reach DTCM and AXI is now image space. Add ASSERTs: image end <= QSPI-copy budget, and an app-region size ASSERT so the image (stored from QSPI 0x90040000) ends strictly below 0x90100000 (excerpt slots, TASK-038.04.01) -> AC2.
2. Selection. build.rs currently include_str!s memory.x; select the file by a cargo feature or env (e.g. LAYOUT=internal|bootloaded, passed through Makefile) and emit rerun-if-env-changed. Keep internal layout default for blinky/ledtest/podtest/panictest (fallback). Add memory-bootloaded.x to firmware/elf-inputs.manifest (name *.x already matches under path; add a 'path memory-bootloaded.x' line) so the provenance digest covers it, and make the layout part of the provenance cfg set in scripts/elf-provenance.sh so a stale internal-layout ELF is not mistaken for a bootloaded one.
3. Makefile. Add LAYOUT variable; bootloaded builds produce <bin>.bin as before but flash target uses 0x90040000 only when LAYOUT=bootloaded (flash path docs are TASK-038.04.07.03, so keep the flash edit minimal: parametrise the address). Targets for main, rig (seed3, seed3+stim-ess, seed3+stim-pulse) and blinky. Update the size-budget comment block.
4. Gates. scripts/check-image-load-addresses.sh: R1/R3 parse the FLASH region from memory.x; make it parse the layout the ELF was built for (read from provenance) so LMA-in-FLASH holds for 0x24000000 and R3 image length = highest LMA end - origin. Add a QSPI-app-region check (image size < 0x100000-0x40000 = 768K-ish budget, tied to the 0x100000 slot origin from the same constant used by the excerpt-layout code, do not restate it). Extend --selftest with a bootloaded fixture. Update scripts/gates.sh tiers, gate-costs.sh and check-elf-staleness.sh as needed.
5. Rig size gate: in a scratch worktree apply wip/TASK-038.04.05-excerpt-install (762392c), build rig for all three feature sets in the bootloaded layout, confirm >= 2 KB spare in the target region (AC1), record sizes in implementation notes. Do not merge 762392c.
Verification: make -C firmware build for each binary/feature set under LAYOUT=bootloaded and internal; scripts/check-image-load-addresses.sh and --selftest; project gates tiers (scripts/gates.sh) green. No hardware: on-device timing is TASK-038.04.07.04, flashing docs TASK-038.04.07.03 - do not tick HUMAN criteria.
Risks: bootloader copies ~480 KB so DTCM-only data; the DMA buffers in D2 and linker INSERT ordering; cortex-m-rt SP = RAM origin+length (DTCM 128K -> 0x20020000, matches trial).
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
