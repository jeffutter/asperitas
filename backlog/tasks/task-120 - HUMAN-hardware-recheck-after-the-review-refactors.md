---
id: TASK-120
title: 'HUMAN: hardware recheck after the review refactors'
status: To Do
assignee:
  - '@human'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - hardware
dependencies:
  - TASK-082
  - TASK-085
  - TASK-087
  - TASK-088
  - TASK-093
  - TASK-094
  - TASK-101
  - TASK-106
priority: medium
type: chore
ordinal: 222800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several review tickets touch bench-verified firmware paths. After they land, walk this checklist at the bench with the agent: flash main and listen (both knobs, no zipper noise, survives a power cycle); flash rig and run a capture (RIGCFG/STATUS/CAPSTAT as in TASK-059.03: sent advancing, dropped_full=0, overrun=0); asperitas-rig query and restart against both main and rig; panictest (LED solid red, panic reaches USB); podtest encoder and buttons (10 detents each way, 10 presses each); LED 1 boot, running and panic states.

Applies to: effect, fw_lib, fw_rig_split, fw_knobstate, fw_ctl, log_commit, log_init, pod_decoder, rgb_led, rig_session (as they land).

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: main is heard with both knobs working and survives a power cycle
- [ ] #2 HUMAN: a rig capture shows STATUS sent advancing, dropped_full=0, overrun=0, and query/restart answer on main and rig
- [ ] #3 HUMAN: panictest reaches the panic LED and USB; podtest encoder and buttons count correctly
- [ ] #4 HUMAN: any regression is filed as a bug ticket with the capture
<!-- AC:END -->
