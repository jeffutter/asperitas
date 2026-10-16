---
id: TASK-086
title: 'Firmware: split rig.rs (2.4k lines) into modules under src/bin/rig/'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - firmware
dependencies:
  - TASK-085
priority: medium
type: chore
ordinal: 188800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rig.rs is 2423 lines, about 948 of them comments. Sections: preamble 57-137, geometry gates 139-506, ring/Producer 314-743, timeline 908-985, dump 987-1111, excerpt install 1113-1419, fx_ctl 1511-1588, excerpt_play 1590-1882, time base 2061-2202, main 2208-2423.

Change: src/bin/rig/main.rs plus capture.rs (Ring, Producer, BLOCK_STATE, timeline, judge), dump.rs, install.rs, replay.rs, fx_ctl.rs and timebase.rs (or the shared lib's). A pure move; keep the const-asserts and behaviour identical.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rig.rs is split into modules; no module over ~600 lines
- [ ] #2 rig images are byte-compatible in behaviour: the same RIGCFG/STATUS/CAPSTAT output on a rig capture (hardware recheck ticket)
<!-- AC:END -->
