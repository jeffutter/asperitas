---
id: TASK-138
title: >-
  Hardware recheck: rig images after the rig.rs module split report the same
  RIGCFG/STATUS/CAPSTAT
status: To Do
assignee:
  - '@human'
created_date: '2026-10-19 16:54'
labels:
  - firmware
  - hardware
dependencies:
  - TASK-086
priority: medium
ordinal: 264800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-086 split firmware/src/bin/rig.rs into modules under firmware/src/bin/rig/ as a pure move. Host-side evidence: clippy clean on every rig variant, every variant builds and grades, and image sizes moved by under 0.5 percent from inlining. Only a board can show the behaviour is unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Flash the bootloaded rig image and capture a session; RIGCFG, STATUS and CAPSTAT match a capture from before the split (same fields, same values for the same build).
- [ ] #2 HUMAN: A stim-excerpt rig image installs and replays a slot, reporting EXCPLAY with clean=1.
<!-- AC:END -->
