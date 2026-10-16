---
id: TASK-112
title: >-
  Repo: one captures layout, relative paths in reports, zstd the text captures,
  size policy
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - repo
  - needs-decision
dependencies: []
priority: low
type: chore
ordinal: 214800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
audio/captures/ holds date-named .console.zst files (31 MB, 19 files) and top-level captures/ holds task-named uncompressed .txt/.json (10 MB, files up to 2 MB); neither is in the README tree; audio/README.md lists 1 of the 19 captures; there is no .gitattributes or LFS. 31 MB of already-compressed captures arrived in plain git in 9 days (loose objects 44 MB against an 8.9 MB pack). Each verdict JSON's 'capture' field holds a /var/folders temp path (e.g. captures/task-019-03-04-soak-snaps-smoothed.json:2), so nothing ties a report to its raw capture except prose.

Change: one tree per ticket (captures/TASK-x/<run>.console.zst + report.json); have asperitas-measure record a repo-relative path; zstd the text captures.
DECISION NEEDED: git LFS, plain git, or fixtures only in git with the rest archived outside the repo? Growth is ~3 MB/day in hardware weeks.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One captures layout documented in the README; reports hold repo-relative capture paths
- [ ] #2 Size policy decided and recorded
<!-- AC:END -->
