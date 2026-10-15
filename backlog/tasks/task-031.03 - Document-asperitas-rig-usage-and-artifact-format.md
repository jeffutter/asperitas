---
id: TASK-031.03
title: Document asperitas-rig usage and artifact format
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 04:26'
updated_date: '2026-10-15 04:43'
labels:
  - task
  - planned
dependencies:
  - TASK-031.02
parent_task_id: TASK-031
priority: medium
ordinal: 154800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Document rig runner usage, exit codes, threshold flag, artifact format and the convention of committing artifacts with hardware verification tickets, in the place the other host tooling is documented (docs/ and CLAUDE.md pointer if appropriate). Covers AC #6 of TASK-031. Depends on the CLI sub-ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Doc covers usage, exit codes, threshold flag, artifact format and commit convention
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-031.03 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added docs/reference/asperitas-rig.md (usage, flags, exit codes 0/1/2, --max-drops, artifact format, commit convention) and a CLAUDE.md pointer. Flags and exit codes cross-checked against asperitas-rig --help and src/main.rs, cli.rs, lib.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Documented asperitas-rig in docs/reference/asperitas-rig.md with a CLAUDE.md reference entry.
<!-- SECTION:FINAL_SUMMARY:END -->
