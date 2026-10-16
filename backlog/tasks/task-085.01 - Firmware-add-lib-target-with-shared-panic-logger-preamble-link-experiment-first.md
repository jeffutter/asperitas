---
id: TASK-085.01
title: >-
  Firmware: add lib target with shared panic/logger preamble (link experiment
  first)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:00'
updated_date: '2026-10-16 20:08'
labels:
  - task
  - firmware
  - planned
dependencies: []
parent_task_id: TASK-085
priority: high
ordinal: 224800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add firmware/src/lib.rs and move the ~50-line panic_handler/_defmt_panic/no-op-logger preamble repeated in the six binaries (blinky, ledtest, main, panictest, podtest, rig) into it, linked per binary with 'use asperitas_firmware as _;'. Do a link experiment first: confirm a library-provided #[panic_handler] and global_logger link under every feature combination CI builds (default, --no-default-features seed3 log-defmt, seed3 only), since defmt_rtt already provides a global_logger from a library. Acceptance: no per-binary preamble remains; panictest still reaches LED and USB (verified in TASK-120); all CI firmware builds link; scripts/gates and Makefile still pass.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 2bba9b9. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Link experiment (blinky first, then all bins): a lib-defined #[panic_handler], #[defmt::panic_handler] and no-op #[defmt::global_logger] link and survive under default, '--no-default-features --features seed3,log-defmt' and '--features seed3'. nm shows one rust_begin_unwind and one _defmt_write/_acquire/_release per ELF; the old 'lib global_logger is dead-code-eliminated' comment is wrong when the binary has 'use asperitas_firmware as _;'. Deviation from the plan: ledtest keeps its own self-contained preamble because its panic handler deliberately avoids asperitas_logging (LED/USB), and a second handler in the lib would collide. Other five bins (blinky, main, panictest, podtest, rig) use the lib. rig only links with ASPERITAS_LAYOUT=bootloaded (flash size), as the gates already do. elf-inputs.manifest already covers src/. scripts/gates.sh push passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added firmware/src/lib.rs holding the shared panic handler, defmt panic handler and defmt logger; blinky, main, panictest, podtest and rig dropped their ~50-line preambles for 'use asperitas_firmware as _;'. ledtest intentionally stays self-contained. All CI feature combos link; gates push tier passes.
<!-- SECTION:FINAL_SUMMARY:END -->
