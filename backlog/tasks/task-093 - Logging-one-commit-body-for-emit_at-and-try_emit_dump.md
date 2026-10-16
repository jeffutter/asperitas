---
id: TASK-093
title: 'Logging: one commit body for emit_at and try_emit_dump'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
dependencies: []
priority: medium
type: chore
ordinal: 195800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
emit_at and try_emit_dump duplicate take_seq, encode, write_whole against LOG_PIPE, the same three-arm counter match and the (outcome, Violation) return (lib.rs:499-527 vs 656-694). lib.rs has 401 comment lines against 311 code; the try_emit_dump doc alone is ~60 lines (582-640) restating emit_at and commit_records.

Change: one private commit(level, now_ms, body, Policy) where Policy says whether there is a reserve (0 logs, RESERVE dumps) and whether a refusal spends a seq and counts as a drop; keep one explanation of the lock rules there. This is the bench-verified commit path.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 emit_at and try_emit_dump share one commit function
- [ ] #2 Golden/unit tests pass unchanged; wire bytes identical
- [ ] #3 Rig capture plus dump re-run per the TASK-038.05 procedure (hardware recheck ticket)
<!-- AC:END -->
