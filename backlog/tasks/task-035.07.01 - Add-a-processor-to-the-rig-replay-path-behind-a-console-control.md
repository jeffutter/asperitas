---
id: TASK-035.07.01
title: Add a processor to the rig replay path behind a console control
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 20:52'
updated_date: '2026-10-15 21:06'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-035.07
priority: medium
ordinal: 170800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
firmware/src/bin/rig.rs replays QSPI excerpts straight to the DAC with no asperitas-dsp processor in path. Add a Processor (Gain first, then a filter) to the replay path, selectable and parameterized via a rig console control, default identity/bypass so existing captures are unchanged. Must build for thumbv7em and host tests for the control parsing must pass. Compiling only; no device claims.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Replay path runs the selected asperitas-dsp Processor with parameters set by a rig control; default is bit-exact bypass
- [x] #2 Control parsing/selection has host tests; firmware builds for target
- [x] #3 docs/reference/asperitas-rig.md documents the control
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 1498cff. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added asperitas_dsp::ReplayFx (bypass/gain/low-pass, host-tested incl. bit-exact bypass), CTL FX f=<2hex> v=<4hex> in asperitas-logging control grammar (plus ErrorCode::Busy), handler in rig run_install, applied in ExcerptPlayer::process_block after take_block. FX must be sent in the 2 s window before capture arm; refused with busy after. No host rig subcommand yet. Compile only; bootloaded stim-excerpt build links.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replay path runs a selectable ReplayFx set by CTL FX; default bypass is bit-exact. Grammar and ReplayFx host-tested, rig builds for thumbv7em (bootloaded), docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
