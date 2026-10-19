---
id: TASK-088
title: 'Firmware: one control-channel dispatcher for every binary'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 17:02'
labels:
  - review-2026-10
  - firmware
  - planned
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
- [x] #1 asperitas-rig query and restart work against a rig image (hardware recheck ticket)
- [x] #2 SET/REL/scaling logic exists once
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by b484188. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
SET/REL/scaling (wire_to_level) now live once in host_control::handle_control; rig's fx_ctl override table and handler arms deleted; rig calls capture_reset_reason first in main. fx_chain.rs has no u16 scaling site, nothing to change. Gates push pass; clippy clean for main, podtest, rig (sine, excerpt, ess, pulse). AC1 is code-complete (VER/RSN/RST now reach the shared dispatcher in rig); on-hardware confirmation is TASK-120's recheck.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved CTL command application into host_control::handle_control (async, FX hook); main/podtest control_task and the rig install loop both use it, with one override table.
<!-- SECTION:FINAL_SUMMARY:END -->
