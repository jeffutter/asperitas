---
id: TASK-102
title: Share level<->wire and FX name mappings
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - host
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
- [ ] #1 One level<->wire implementation used by firmware, rig and measure
- [ ] #2 FxKind::from_name replaces the hardcoded names in measure
<!-- AC:END -->
