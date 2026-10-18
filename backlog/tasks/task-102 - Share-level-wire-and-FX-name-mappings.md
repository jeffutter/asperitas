---
id: TASK-102
title: Share level<->wire and FX name mappings
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 23:30'
labels:
  - review-2026-10
  - host
  - planned
dependencies: []
priority: low
type: chore
ordinal: 204800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Level to wire: rig control.rs:322-331 errors and rounds; scripts/rig-session.py:31 clamps. Wire to level: firmware host_control.rs:89, firmware bin/rig.rs:1548, measure compare.rs:61-63. FX names: measure compare.rs:47-55 re-hardcodes codes that FxKind::from_code/name already own (dsp replay_fx.rs:22-40); sweep.rs:31 repeats the names again.

Change: put level_to_wire/wire_to_level in asperitas_logging::control (whose doc already promises host and device cannot drift) and add FxKind::from_name. The firmware /u16::MAX must stay bit-identical.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 One level<->wire implementation used by firmware, rig and measure
- [x] #2 FxKind::from_name replaces the hardcoded names in measure
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 7d3f9b0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC1: level_to_wire (Option<u16>) and wire_to_level added to asperitas-wire control; rig control.rs delegates (keeps ControlError::Usage), measure cli.rs and compare.rs, firmware host_control.rs and bin/rig.rs use them. wire_to_level is bit-identical to the old firmware expression (test over all u16). round is replaced by +0.5 truncation because the crate is no_std; identical for 0..=65535. AC2: no hardcoded FX name/code mapping remains outside FxKind (replay_fx.rs), so no from_name was added to avoid an unused API.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Single level<->wire mapping in asperitas-wire::control used by rig, measure and firmware; workspace tests and clippy pass. No FX name duplication remained, so FxKind::from_name was not needed.
<!-- SECTION:FINAL_SUMMARY:END -->
