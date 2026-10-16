---
id: TASK-088
title: 'Firmware: one control-channel dispatcher for every binary'
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
ordinal: 190800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two control dispatchers and two override tables: host_control.rs:81-140 (two knobs) and rig.rs:1262-1311 fx_ctl::KNOB (one knob). SET/REL handling and u16-to-f32 scaling are written three times (host_control.rs:89, rig.rs:1548, fx_chain.rs:62). rig answers VER/RSN/RST with unknown-verb (rig.rs:1302) and never calls capture_reset_reason, so asperitas-rig cannot query or restart a rig image.

Change: one dispatcher handling VER, RSN, RST, SET and REL, with a hook for binary-specific verbs (FX, excerpt); share the override table once the effect ticket lands.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 asperitas-rig query and restart work against a rig image (hardware recheck ticket)
- [ ] #2 SET/REL/scaling logic exists once
<!-- AC:END -->
