---
id: TASK-085.03.01
title: >-
  Firmware: audio module in lib (encode_block, SAI1 executor, start(),
  AUDIO_EXIT enum) and migrate main onto it
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:20'
updated_date: '2026-10-16 20:29'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-085.03
priority: high
ordinal: 228800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add firmware/src/audio.rs to the lib: encode_block once, the SAI1 InterruptExecutor + vector + P6 priority + spawn, one start(interface, callback) with a single failure policy (log, set LED, keep serving the console - not main's spin that kills the console), one AUDIO_EXIT enum. Migrate firmware/src/bin/main.rs onto it. Delete scripts/check-main-audio-executor.sh and its references in scripts/gates.sh and docs. Acceptance: main calls audio::start; executor setup and encode_block exist once in the lib; script and gate references removed; builds and gates pass.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-085.03.01. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added firmware/src/audio.rs (decode/encode_block, AudioExit enum + AUDIO_EXIT/AUDIO_BLOCKS, SAI1 InterruptExecutor, AudioProcessor trait, async start()). Embassy tasks cannot be generic, so the callback is a &'static mut dyn AudioProcessor (main owns a StaticCell<KnobChain>). The SAI1 vector is a lib macro audio_sai1_vector! invoked by main, because a lib-level #[interrupt] SAI1 duplicates rig's own vector at link (verified). TASK-085.03.02 can then invoke the macro in rig. start() on failure logs, records AUDIO_EXIT|0x10, sets LED Panicked and returns, so main keeps serving the console. Boot order change: LED Running and knob task spawn now precede start_interface. Removed check-main-audio-executor.sh, its gate, gate-costs entry and counts, doc Guard bullet. Verified: main/blinky/podtest/panictest/ledtest build, rig bootloaded builds, scripts/gates.sh push green. Hardware recheck is TASK-120.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
main now calls audio::start from the new lib audio module; executor, encode_block and exit enum exist once in the lib; guard script and gate removed.
<!-- SECTION:FINAL_SUMMARY:END -->
