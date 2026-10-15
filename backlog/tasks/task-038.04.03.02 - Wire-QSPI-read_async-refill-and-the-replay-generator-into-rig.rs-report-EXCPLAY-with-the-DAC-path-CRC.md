---
id: TASK-038.04.03.02
title: >-
  Wire QSPI read_async refill and the replay generator into rig.rs, report
  EXCPLAY with the DAC-path CRC
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:39'
updated_date: '2026-10-15 17:48'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03.01
  - TASK-038.04.05
parent_task_id: TASK-038.04.03
priority: high
ordinal: 138800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: firmware/src/bin/rig.rs plus Cargo feature (e.g. stim-excerpt, mutually exclusive with the other generators via the existing GENERATORS_SELECTED assert) and slot choice. Staging buffers in internal RAM; refill task on the thread-mode executor calls Flash::read_async over MDMA (bindings shared with the install path from TASK-038.04.05); the SAI1 callback only pops from the ping-pong core and never touches QSPI. Must resolve D-cache coherency for MDMA writes into cacheable AXI SRAM (invalidate after refill or place staging in a non-cacheable region) and state which. After one full pass emit one console record (EXCPLAY slot, bytes, crc16, underruns) built via the one whole-record emit path; code and docs state that this CRC covers only what was handed to the DAC encoder and says nothing about the codec or cable (TASK-035 AC #4 posture). Confirm callback max_block_us headroom claim is not worsened (about 590 us of 666 us today) and add a CI build of the new feature. No bench claim. Depends on the staging core and on TASK-038.04.05.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Cargo feature stim-excerpt added to firmware/Cargo.toml, counted by GENERATORS_SELECTED so it is mutually exclusive with stim-sine/ess/pulse; slot chosen at build time from ASP_RIG_EXCERPT_SLOT (const-parsed like ASP_RIG_CAPTURE_SECONDS, malformed or >= SLOT_COUNT fails the build)
- [x] #2 An ExcerptPlayer generator implementing Processor and Stimulus: the SAI1 callback path only calls ReplayCore::take_block (TASK-038.04.03.01), converts s16 to f32 as s/32768 into both output channels, and touches no QSPI, flash mutex, await or allocation; describe() reports slot, bytes and header crc16 within the RIGGEN 160-byte budget
- [x] #3 A thread-mode refill task claims an Empty half, calls Flash::read_async over the shared Flash mutex (TASK-038.04.05) for the next HALF_BYTES of the slot's PCM, then finish_fill; staging statics live in AXI SRAM, 32-byte aligned; after each read the task invalidates the D-cache range if SCB::dcache_enabled() is true, and a comment states the coherency argument for both cache states
- [x] #4 Boot reads and validates the slot header (async) before the generator is built; an empty or invalid slot halts with a console record and a red LED rather than playing silence; the header's pcm crc16 is the 'want' value
- [x] #5 On pass completion exactly one EXCPLAY record is emitted through the whole-record emit path: slot, bytes, crc16 of samples handed to the DAC encoder, underruns, clean flag, and a want= field with the stored crc; code comment and record docs state the CRC says nothing about what returns through codec and cable (TASK-035 AC #4 posture)
- [x] #6 rig builds for thumbv7em with stim-excerpt in both log-usb and log-defmt configurations, CI builds the feature, clippy clean, other stimulus builds unchanged; callback worst-case instruction cost added by the replay path is estimated in a comment against the ~590 us headroom; no bench claim (TASK-038.09)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit that carries Task-Id TASK-038.04.03.02 (stim-excerpt wiring). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in firmware/src/bin/rig.rs (module excerpt_play, plus halt_unplayable), firmware/Cargo.toml (stim-excerpt), crates/asperitas-logging/src/console.rs (EXCPLAY and EXCPLAYFAIL bodies, host-tested with a saturated-length compile-time check). Nothing was shipped before: no prior stim-excerpt or EXCPLAY code existed.

Deviations from the plan, with reasons: (1) Staging placement. The plan assumed default .bss is AXI SRAM, but under memory-bootloaded.x (the only layout rig links in now) .bss is DTCM. DTCM is never cached and MDMA reaches it (ST QSPI-over-MDMA examples do the same with default H7 layouts), so no cache maintenance applies there; fill_half still invalidates when SCB::dcache_enabled() for the AXI case, and the section comment states the argument for both cache states and both regions. Not verified on a board. (2) The player is one-shot (one_shot_samples = Some), so the audio task holds it silent until the capture's first callback and STIMSTART says where it began; reset() is a no-op because the pass is started and both halves staged in main before prepare_interface. (3) REPLAY is a const-initialised static whose CRC seed is non-zero, so it lands in .data and the bootloaded image grew about 16 KB (160,265 B vs 144,265 B for sine); spare is 331 KB. (4) EXCPLAYFAIL (why=empty|invalid) is a new verb for AC4, separate from EXCPLAY so EXCPLAY stays exactly-once-per-pass. (5) An EXCSTART is now refused while a replay pass is open (boot until completion), via the existing Why::Busy path. (6) Refill is a 2 ms poll, asserted at compile time to be at most half the 10 ms scheduling latency in the staging table.

CI: rig:seed3,stim-excerpt added to scripts/check-bootloaded-builds.sh (console build, load-address graded), new push gate rig-stim-excerpt-rtt-build (RTT-only, ASP_RIG_EXCERPT_SLOT=13), and two commit-tier clippy gates. docs/gate-costs.json refreshed with GATE_COSTS_BOOTSTRAP=1 (the new gates had no entries); all three tiers passed during the refresh.

Callback cost estimated in a comment at about 8 us worst case (bitwise CRC-16 over 64 bytes dominates) against the ~590 us headroom; it is an estimate, not a reading. Hardware not available: audibility, observed max_block_us, QSPI refill margin and CRC match are left to TASK-038.09. Host-side rig tooling does not yet know RIGGEN name=excerpt or the EXCPLAY verbs; that is for TASK-038.09 or a follow-up.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the stim-excerpt rig generator: ExcerptPlayer pops 32-sample blocks from ReplayCore in the SAI1 callback (no QSPI, mutex or await), a thread-mode task refills halves with Flash::read_async over the shared FLASH mutex with D-cache invalidation when enabled, boot validates the slot header and stages both halves before audio (blank or invalid slot sends EXCPLAYFAIL, red LED, no audio), and one EXCPLAY record (slot, bytes, DAC-path crc16, want, underruns, clean) is emitted per pass. ASP_RIG_EXCERPT_SLOT is const-parsed. Builds and clippy pass for log-usb and log-defmt; CI/gates extended and the gate-cost ledger refreshed. No bench claim; hardware verification is TASK-038.09.
<!-- SECTION:FINAL_SUMMARY:END -->
