---
id: TASK-038.04.07.03
title: Update flashing paths and docs for the bootloaded app
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-09 02:25'
labels:
  - task
dependencies:
  - TASK-038.04.07.01
parent_task_id: TASK-038.04.07
priority: medium
ordinal: 142800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Update make flash targets, the probe-flash path (app no longer in internal flash), README and docs/reference/daisy-seed3.md flashing sections for the bootloaded layout, using the addresses proven in the bench ticket. Note in CLAUDE.md-referenced docs that the 128 KB internal-flash wall no longer applies and record the measured rejected routes (opt-level s 116,856 B, z 111,548 B, per-package overrides, rig split) from the parent description with sizes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 make flash targets and probe path documented and dry-run verified (make -n) for the bootloaded layout
- [ ] #2 README and daisy-seed3.md flashing sections updated; rejected routes recorded with measured sizes
<!-- AC:END -->
