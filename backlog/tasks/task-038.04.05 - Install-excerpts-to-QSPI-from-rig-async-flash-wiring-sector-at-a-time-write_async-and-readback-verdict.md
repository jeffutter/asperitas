---
id: TASK-038.04.05
title: >-
  Install excerpts to QSPI from rig: async flash wiring, sector-at-a-time
  write_async and readback verdict
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-15 17:19'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.02
  - TASK-038.04.04
  - TASK-038.04.07
parent_task_id: TASK-038.04
priority: high
ordinal: 135800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: firmware/src/bin/rig.rs plus Cargo wiring. Build daisy_embassy Flash via build_async with an MDMA channel and bind QUADSPI and MDMA interrupts (see examples/flash.rs in the vendored daisy-embassy). Consume install records from the inbound path, write each sector with write_async only at sector-aligned slot addresses, then read the slot back in chunks, compute length and CRC-16, and emit EXCOK or EXCFAIL. The blocking flash API must not appear anywhere in the binary (unbounded busy loop); async timeouts panic, so state the consequence. Show that erase pauses (about 47 sectors, tens of seconds) do not disturb the audio interrupt executor or the SDRAM capture ring: a host-computable argument plus rate gates where possible. Acceptance: rig builds and CI covers it; no device claim (TASK-038.09 owns the bench). Covers parent AC #2 (device half) and AC #3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 rig builds Flash<Async> via board.flash.build_async(MDMA_CH0, RigFlashIrqs) with QUADSPI and MDMA bound in a new bind_interrupts struct; the Flash lives in a static embassy_sync Mutex so TASK-038.04.03.02 can share it; no blocking read/write/erase call and no use of FlashBuilder::build anywhere in the binary (grep-enforced in CI)
- [x] #2 An install task consumes records from usb::inbound() (TASK-038.04.02), drives excerpt::Installer (TASK-038.04.04) and calls write_async only with exactly 4096 bytes at sector-aligned addresses inside the target slot; an assert at the single call site checks alignment, length and slot bounds before the driver can erase anything
- [x] #3 After EXCEND the task reads the PCM back in bounded chunks with read_async, computes length and CRC-16, calls Installer::verdict, writes the header sector only on success, and emits EXCOK or EXCFAIL through the one whole-record emit path; a truncated or corrupted upload cannot produce EXCOK (covered by the Installer host tests; device path reviewed against them)
- [x] #4 Erase pauses do not disturb capture: the install task runs on the thread-mode executor only, the SAI1 callback never references the flash, and a written argument in a comment prices the MDMA and QUADSPI ISR preemption of the P6 audio executor against callback headroom (about 590 us of 666 us); install is refused while capture is armed or dumping, with a console record saying so
- [x] #5 CI builds rig with the install path on thumbv7em and passes cargo clippy; no bench claim is made - TASK-038.09 owns the hardware check
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED in the commit whose subject is 'TASK-038.04.05: install excerpts to QSPI from rig via async flash' (git log --grep TASK-038.04.05). This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Blocked 2026-10-08 on TASK-038.04.07 (internal flash budget). The implementation is written and parked on branch wip/TASK-038.04.05-excerpt-install, commit 762392c (one commit on top of 1ef8714; its commit-tier gates passed, 15 gates). It covers every AC in code: build_async(MDMA_CH0, RigFlashIrqs) into static FLASH: Mutex<ThreadModeRawMutex, Option<Flash<Async>>>; QUADSPI and MDMA set to P7 (below SAI1's P6) and read back in the nvic info line; run_install consumes usb::enable_inbound(), drives excerpt::Installer, one guarded rewrite_sector call site (sector-aligned, inside the EXCSTART slot, [u8; 4096] by type) for write_async/erase_async; Verify reads back in 4096-byte read_async chunks, CRC-16, Installer::verdict, header sector written last on Ok, EXCOK/EXCFAIL via emit_console; CAPTURE_ACTIVE (arm..DUMPEND) refuses an EXCSTART and aborts an open install with EXCFAIL why=busy (new Why::Busy + Installer::abort in asperitas-logging, with host tests); firmware/build.rs rig_flash_ban fails the build on blocking .read(/.write(/.erase(/.read_uuid(/.build() in rig.rs (verified: a comment passes, a real call fails). Clippy -D warnings is clean in both the console and RTT-only configs.

What blocks it: rig does not link. Internal flash is 131,072 B; the patched images are 143,312 (seed3), 145,104 (stim-ess), 143,536 (stim-pulse); baseline seed3 at 1ef8714 is 125,016. Measured by linking against a 256K FLASH region in a scratch memory.x. The ~6 KB free at baseline cannot hold even a minimal install path (~10 KB irreducible: parse_record, base64 decode, Installer, QSPI/MDMA driver, header encode, CRC table), so room has to be made elsewhere, and the options (opt-level, splitting rig into its own package, libm sin) touch the audio callback codegen TASK-038.05 measured - a decision for its own ticket, not a side effect of this one.

Next step: once TASK-038.04.07 lands, rebase wip/TASK-038.04.05-excerpt-install onto main, confirm all three rig builds link under budget, run the push tier, then tick the ACs here. The main checkout was left clean (the work was reverse-applied after the branch commit).

Landed from parked branch wip/TASK-038.04.05-excerpt-install (762392c), cherry-picked onto main after TASK-038.04.07. Conflicts resolved in firmware/build.rs (kept provenance(layout), version(), rig_flash_ban()) and rig.rs (RING_FILL log kept, CAPTURE_ACTIVE set at arm, join3 with install_fut). rig links only under ASPERITAS_LAYOUT=bootloaded (143,529 / 145,449 / 143,753 B against 491,520; internal layout overflows by ~12 KB), so scripts/gates.sh rig-* push builds now set that layout and the console cross-compile gate names its bins and omits rig; the RTT-only rig still fits internal flash. Verified: bootloaded matrix script passes, clippy -D warnings clean in console and RTT cfgs, asperitas-logging host tests pass. No bench claim; TASK-038.09 owns hardware.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig now installs excerpts to QSPI: Flash<Async> via build_async(MDMA_CH0), guarded 4096-byte sector-aligned write_async call site, chunked readback with CRC-16 and Installer::verdict, header sector last, EXCOK/EXCFAIL, refusal while a capture is active (why=busy), and a build.rs ban on blocking flash calls. Builds in the bootloaded layout; gates adjusted accordingly. Not exercised on hardware.
<!-- SECTION:FINAL_SUMMARY:END -->
