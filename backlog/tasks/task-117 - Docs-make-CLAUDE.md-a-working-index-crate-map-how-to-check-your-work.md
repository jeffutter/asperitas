---
id: TASK-117
title: 'Docs: make CLAUDE.md a working index (crate map, how to check your work)'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-16 18:14'
labels:
  - review-2026-10
  - docs
dependencies:
  - TASK-114
priority: medium
type: chore
ordinal: 219800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CLAUDE.md:9-31 lists 4 docs and omits doc-001 (the plan), idea.md, audio/README.md, seed3-schematic-search-log.md and docs/upstream/; there is no crate map, no build/test entry point (bash scripts/gates.sh commit|push, cd firmware && make ...), and no pointer to where captures go. Plus the stale TASK-037 caveat (covered by the probe docs ticket).

Change: add a ~10-line 'where things live / how to check your work' block.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CLAUDE.md links every doc and gives the crate map and the commands to check work
<!-- AC:END -->
