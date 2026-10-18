---
id: TASK-099.04
title: Retire scripts/rig-session.py and repoint docs and references
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-18 20:08'
labels:
  - task
  - planned
dependencies:
  - TASK-099.03
parent_task_id: TASK-099
priority: medium
ordinal: 248800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Delete scripts/rig-session.py, repoint docs/reference/asperitas-rig.md (lines mentioning it) and any gates/lefthook references to the session subcommand. AC: file deleted, grep for rig-session finds no live reference.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-099.04. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Deleted scripts/rig-session.py; repointed Timing bullet and Measured section in docs/reference/asperitas-rig.md to asperitas-rig session. grep for rig-session outside backlog finds nothing. TASK-125 is moot now that the file is gone (left open for the owner to close).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Retired scripts/rig-session.py and repointed docs to the session subcommand.
<!-- SECTION:FINAL_SUMMARY:END -->
