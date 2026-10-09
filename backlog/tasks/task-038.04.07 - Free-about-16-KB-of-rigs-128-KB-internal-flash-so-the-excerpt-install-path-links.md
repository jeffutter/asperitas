---
id: TASK-038.04.07
title: >-
  Free about 16 KB of rig's 128 KB internal flash so the excerpt install path
  links
status: Blocked
assignee:
  - '@human'
created_date: '2026-10-08 16:39'
updated_date: '2026-10-09 02:25'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.07.01
  - TASK-038.04.07.02
  - TASK-038.04.07.03
  - TASK-038.04.07.04
parent_task_id: TASK-038.04
priority: high
ordinal: 139800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-038.04.05's install path (async QSPI flash driver, inbound record parse, Installer, readback, EXCOK/EXCFAIL) is written and lints clean on branch wip/TASK-038.04.05-excerpt-install (commit 762392c), but rig no longer links: the STM32H750's internal flash is one 131,072-byte sector and the images come out at 143,312 B (seed3), 145,104 B (seed3,stim-ess) and 143,536 B (seed3,stim-pulse). Baseline at 1ef8714 is 125,016 B for seed3, so the install path costs ~18.3 KB at opt-level 3; stim-ess overflows by 14,032 B. Measured 2026-10-08 by linking the patched tree against a 256K FLASH region in a scratch copy of memory.x and summing .vector_table+.text+.rodata+.data with rust-size.

Where the bytes go (symbol diff baseline -> patched, seed3): the Join3 state machine of report_capstat/run_capture/run_install +6.1 KB, excerpt::parse_record 1.2 KB, rewrite_sector future 1.2 KB, Installer::feed 1.0 KB, dump::decode 0.9 KB, build_async + Qspi::new_bank1 1.5 KB, SlotHeader::encode 0.5 KB, CRC table 0.5 KB, EXCOK/EXCFAIL bodies 0.75 KB, flash erase/write/poll futures ~1.4 KB. Even a tight rewrite of the install loop leaves ~10 KB irreducible, against ~6 KB free at baseline (stim-ess less), so this needs room made elsewhere, not just a smaller install path.

Measured options (same patched tree): whole-workspace opt-level "s" -> 116,856 B; opt-level "z" -> 111,548 B; [profile.release.package.asperitas-logging] opt-level="s" alone -> 131,904 B (still 832 over); adding embassy-usb + embassy-usb-synopsys-otg at "s" -> 132,720 B (worse); embassy-stm32 at "s" -> 142,064 B. Per-package overrides barely move because the generic async code is monomorphized into the bin crate. Other candidates not yet measured: move rig into its own firmware package so a [profile.release.package] override shrinks only rig; shrink the install loop to one flash call site; libm f64 sin/rem_pio2_large (~5.5 KB) in the sine generator; core::str::count::do_count_chars (1.3 KB, padded str formatting somewhere); the Daisy bootloader route (app in QSPI/SRAM) from docs/reference/daisy-seed3.md.

Trade-off the plan must weigh: any opt-level change alters the codegen of the audio callback whose timings (max_block_us, worst_gap_us) TASK-038.05 measured on the bench at opt-level 3, and it changes every binary in the workspace unless rig is split out. If the chosen route changes callback codegen, record that the bench numbers predate it and split a @human re-measure subtask.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rig builds for seed3, seed3,stim-ess and seed3,stim-pulse with the install path from wip/TASK-038.04.05-excerpt-install (762392c) applied, each with at least 2 KB of internal flash to spare, sizes recorded in the notes
- [ ] #2 The chosen route and the rejected ones are written down with measured sizes; if callback codegen changes, the notes say the TASK-038.05 timings predate it and a @human re-measure subtask exists
- [ ] #3 All gates tiers that build firmware pass; no other binary's behaviour changes except as the chosen route states
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against e6c6557

Approach: owner chose the Daisy bootloader route (app in QSPI/AXI SRAM) over opt-level or binary-split. Because it is unproven on Seed3, the work is gated on a bench proof.

Order: .01 (@human bench proof of bootloader on Seed3) -> .02 (@agent link layout, build targets, gates; rig+762392c sizes with >=2 KB spare) and .03 (@agent flashing paths and docs) -> .04 (@human re-measure callback timing, since codegen placement changes and TASK-038.05 numbers predate it).

If .01 fails, stop and re-decide with the owner; the measured fallback sizes are in the description (opt-level s 116,856 B, z 111,548 B).

Parent is @human (inherits strictest child) and is a pure tracking ticket; AC #1-#3 are satisfied by .02 (sizes), .03 (route write-up) and .04 (timing predates note). After close, rebase wip/TASK-038.04.05-excerpt-install (762392c) and unblock TASK-038.04.05. Note .02's unattended execution must wait for .01 Done via dependency.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Owner decision, 2026-10-08: take the Daisy bootloader route

The owner chose option 3 of three put to them (see the 2026-10-08 session): move firmware onto the Daisy bootloader, so the app lives in QSPI and runs from internal RAM (about 480 KB), instead of shrinking rig with opt-level s/z or splitting the install path into its own binary. Rationale: the 128 KB wall is structural. main will hit it once real DSP lands, opt-level s only buys 12-16 KB that the replay work (.03.02) would spend, and opt-level changes would invalidate the bench-measured callback timing. No size-optimisation stopgap was requested.

For the planner:
- **Unproven on Seed3:** libDaisy has no Seed3 support, and the Seed3's USB-C differs from earlier Seeds. Whether the stock DaisyBootloader binary enumerates over USB-C and drives this QSPI part is the first thing to establish, and it needs the board: split a @human bench subtask. Per CLAUDE.md that makes this ticket @human too, and TASK-038.04.05 rightly waits on it.
- **QSPI layout:** the excerpt slots start at QSPI offset 0x100000 (TASK-038.04.01); the bootloader's app region must fit below that, or the layout moves in the same change.
- **Flashing changes for every binary:** the make targets, the probe-flash path (the SDRAM/QSPI app is no longer in internal flash), elf-provenance/load-address gates, and the README and daisy-seed3.md flashing sections.
- **Code placement:** running from AXI SRAM changes where code executes, so rig's callback timing (72 us, TASK-038.05) needs one re-measure once a bootloaded rig runs.
- **Parked work:** the install path is on branch wip/TASK-038.04.05-excerpt-install (762392c) and needs rebasing once this lands.
<!-- SECTION:NOTES:END -->
