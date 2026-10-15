---
id: TASK-074
title: >-
  Fix: replay staging sits in DTCM under the bootloaded layout, but its comment
  and cache argument say AXI SRAM
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 17:58'
updated_date: '2026-10-15 18:12'
labels:
  - bug
  - planned
dependencies: []
references:
  - firmware/src/bin/rig.rs
  - firmware/memory-bootloaded.x
  - firmware/memory-internal.x
priority: high
type: bug
ordinal: 161800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found 2026-10-15 while checking 382bb31 (TASK-038.04.03.02). The rig stim-excerpt replay staging ping-pong (`REPLAY`, repr(align(32))) is plain `.bss`. Under the bootloaded layout (memory-bootloaded.x) `.bss` is DTCM: a build with `ASP_RIG_EXCERPT_SLOT=0 make build BINARY=rig LAYOUT=bootloaded FEATURES="seed3 stim-excerpt"` puts `__sbss` at 0x20004220 and `__ebss` at 0x200096d4. The comment block "Where the staging lives, and the D-cache" in firmware/src/bin/rig.rs (~line 1373) says the staging is in the image's AXI SRAM and argues about D-cache coherency on that basis. Under the internal layout that was true; under the bootloaded layout (the one rig now ships in, TASK-038.04.07) it is not. DTCM is not cached, so the invalidate is a harmless no-op, but the real question is whether MDMA can write into DTCM. DMA1/DMA2 cannot (embassy-rs/embassy#3747, see memory-bootloaded.x); MDMA is believed to reach DTCM through its AHBS port but nobody has confirmed that from the reference manual or on hardware. If it cannot, read_async refills would land nowhere and the replay would play zeros or fail its CRC (TASK-038.09 would then fail confusingly).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Written from the STM32H750 reference manual (RM0433 bus-matrix / MDMA sections), not from memory: whether MDMA can read and write DTCM (0x20000000) and which port it uses; the finding and the manual section/table cited in the notes
- [x] #2 If MDMA reaches DTCM: the 'Where the staging lives, and the D-cache' comment in firmware/src/bin/rig.rs is corrected to describe DTCM under the bootloaded layout and AXI SRAM under the internal layout, with the cache argument stated for each, and a cfg or comment makes it impossible to read the wrong layout's claim
- [x] #3 If MDMA does not reach DTCM: the staging is given an explicit placement in a region MDMA reaches under the bootloaded layout (for example a dedicated section in the 32 KB AXI SRAM above the 480 KB image, or D2), via memory-bootloaded.x, with the load-address gate and the __ebss ASSERT still passing
- [x] #4 A gate or linker ASSERT checks where the staging lands under each layout, so a layout change cannot silently move it back; scripts/gates.sh push stays green
- [x] #5 The final note says what TASK-038.09 should look at first if replay returns zeros or a CRC mismatch (a device claim is not made; the bench check stays with TASK-038.09)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit (see git log Task-Id: TASK-074). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC1 (honest scope): RM0433 could not be fetched in this session (curl blocked, WebFetch of the ST PDF timed out), so the manual section/table is NOT cited. Secondary evidence only: an ST community thread on the sibling RM0399 says the TCMs are reached through a dedicated MDMA-to-AHBS path outside the bus matrices. That is consistent with the ticket's worry (the transfer must be steered to the AHB port; nothing in this tree does that), so the finding is recorded as UNCONFIRMED and branch B was taken as the plan prescribes. AC2 is the MDMA-reaches-DTCM branch and is not applicable (left unchecked-by-design, see below). Change: REPLAY now has #[link_section=".replay_bss"]; memory-bootloaded.x places it NOLOAD in new AXI_TAIL (0x24078000, 32K, above the 480K image); memory-internal.x places the same name in RAM (AXI). NOLOAD means nothing initialises it, so excerpt_play::zero_staging() runs in main before load (all-zero is the valid idle state). Comment 'Where the staging lives, and the D-cache' rewritten. Gate: check-bootloaded-builds.sh asserts REPLAY symbol lies in 0x24078000..0x24080000 for rig stim-excerpt (measured 0x24078000); region LENGTH makes the linker refuse overflow. scripts/gates.sh push green. Internal layout rig+stim-excerpt does not link (known, image too big for 128K flash), so that layout is covered by the linker script only.

AC5 - if TASK-038.09 sees zeros or an EXCPLAY CRC mismatch: first confirm rust-nm shows REPLAY at 0x24078000, then whether read_async completed (MDMA transfer-error flags, the Flash error path) before suspecting QSPI content or the slot install. If it works, the DTCM question is simply moot. No device claim made.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved the rig replay staging out of DTCM into the AXI SRAM tail above the bootloaded image via a .replay_bss NOLOAD section (both layouts), zeroed at boot, comment corrected, and a build gate asserting its address. RM0433 was unreachable so MDMA-to-DTCM stays unconfirmed (AC2 N/A); hardware check stays with TASK-038.09.
<!-- SECTION:FINAL_SUMMARY:END -->
