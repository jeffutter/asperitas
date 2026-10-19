---
id: TASK-112
title: >-
  Repo: one captures layout, relative paths in reports, zstd the text captures,
  size policy
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 03:16'
labels:
  - review-2026-10
  - repo
  - planned
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
- [x] #1 One captures layout documented in the README; reports hold repo-relative capture paths
- [x] #2 Size policy decided and recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-112 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Plain git is fine (not many captures expected). No LFS. Do the layout, repo-relative paths in reports, and zstd the text captures.

Moved all raw captures into captures/TASK-x/ (git mv), reports renamed to <run>.report.json beside their capture, text captures zstd -19 as .txt.zst. asperitas-measure cli.rs report_path records a repo-relative path (nearest .git ancestor), with unit and integration tests. Two exploratory reports have capture=null (raw never committed). Size policy (plain git, no LFS, review at ~100 MB) recorded in audio/README.md. Tests, clippy, fmt, gates commit pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
One captures tree (captures/TASK-x), repo-relative capture paths in reports, text captures zstd-compressed, size policy documented in audio/README.md; code, tests and docs repointed.
<!-- SECTION:FINAL_SUMMARY:END -->
