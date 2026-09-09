---
id: TASK-039
title: >-
  Fix: CLAUDE.md says daisy-embassy PR #80 is unmerged and orders a commit-SHA
  pin
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:58'
labels: []
dependencies: []
modified_files:
  - CLAUDE.md
priority: medium
type: chore
ordinal: 61500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
docs/reference/rust-daisy-stack.md records PR #80 as merged on 2026-08-01T14:20:54Z, and firmware/Cargo.toml tracks branch = "master" with the seed3 feature. The pointer text in CLAUDE.md still says the PR 'is currently unmerged. Pin to its commit SHA. Time-sensitive; re-check before relying on it.' An agent that follows that instruction literally would pin a nonexistent SHA or downgrade to a feature branch. Found while parking TASK-038, whose planning note 4 already flagged it as outside that ticket's diff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CLAUDE.md's rust-daisy-stack bullet states PR #80's real status (merged 2026-08-01, per docs/reference/rust-daisy-stack.md line 40) and drops the instruction to pin an unmerged PR's commit SHA.
- [ ] #2 The wording keeps the reason the bullet exists — the stack's Seed3 support is upstream and time-sensitive — without asserting a fact that is now false.
- [ ] #3 grep -n 'unmerged' CLAUDE.md returns nothing, and no other file in docs/reference/ repeats the stale claim.
<!-- AC:END -->
