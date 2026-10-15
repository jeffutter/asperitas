---
id: TASK-038.04.07.03
title: Update flashing paths and docs for the bootloaded app
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-15 16:25'
labels:
  - task
  - planned
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
- [x] #1 make flash targets and probe path documented and dry-run verified (make -n) for the bootloaded layout
- [x] #2 README and daisy-seed3.md flashing sections updated; rejected routes recorded with measured sizes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-038.04.07.03. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
make -n flash-all BINARY=rig: LAYOUT=internal -> dfu-util -s 0x08000000:leave -D rig.bin; LAYOUT=bootloaded -> cargo objcopy ... rig-bootloaded.bin then dfu-util -s 0x90040000:leave -D rig-bootloaded.bin. probe-flash/probe-run refuse LAYOUT=bootloaded (guard already in Makefile from TASK-038.04.07.02), so no Makefile change was needed. README gained a Bootloaded layout section; daisy-seed3.md gained flashing/probe notes, a note that the 128 KB wall does not apply to bootloaded builds, and a rejected-routes table with measured sizes. scripts/check-doc-artifact-names.sh passes. Booting main/rig bootloaded remains unverified (TASK-038.04.07.04).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Documented the bootloaded flashing path and probe limits in README.md and docs/reference/daisy-seed3.md, recorded rejected routes with measured sizes, and verified the flash targets with make -n.
<!-- SECTION:FINAL_SUMMARY:END -->
