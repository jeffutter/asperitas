---
id: TASK-035.06
title: >-
  Expose measurement analysis as a CLI subcommand and document it with the host
  tooling
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 20:50'
labels:
  - task
  - planned
dependencies:
  - TASK-035.02
  - TASK-035.03
  - TASK-035.04
  - TASK-035.05
parent_task_id: TASK-035
priority: high
ordinal: 167800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an analyze entry point (asperitas-measure binary or asperitas-rig subcommand) that prints metrics JSON and exits 0 pass / 1 failed verdict / 2 error, matching asperitas-rig convention. Document usage, metric definitions, blind spots, and fixture workflow in docs/reference/asperitas-rig.md and the README measurement-rig section. Covers TASK-035 AC7. Depends on all other subtasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-measure analyze prints metrics JSON and exits 0 pass / 1 failed verdict / 2 error
- [x] #2 Integration tests cover exit 0, exit 1 and exit 2 against committed captures
- [x] #3 docs/reference/asperitas-rig.md documents usage, metric definitions, blind spots and fixture workflow; README points to it
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by f4ac22f. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-measure binary (src/main.rs) over src/cli.rs; sine judged against baseline, sweep/pulse alignment only, --excerpt runs compare(). Tests in tests/cli.rs. Docs replaced stub section in asperitas-rig.md plus README pointer. Verified on 2026-10-07 and bootloaded 300 s captures: exit 0.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-measure analyze CLI with JSON report and 0/1/2 exit codes, integration tests, and documentation.
<!-- SECTION:FINAL_SUMMARY:END -->
