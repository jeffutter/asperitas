---
id: TASK-041
title: Ignore .agents/ loop state so git status stays clean
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 19:43'
updated_date: '2026-09-10 21:45'
labels: []
dependencies: []
priority: low
type: chore
ordinal: 66500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The autonomous pi/ralph loop writes .agents/role.json (currently {"agent": "plan"}) into the working tree and nothing ignores it, so every run ends with an untracked-file diff that has to be eyeballed and deliberately left out of commits. .gitignore already carries the sibling case - a .pi/ralph/ entry with a comment explaining that loop runtime state is machine-local while the rest of .pi/ is tracked project config. Add .agents/ alongside it with the same reasoning.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 .gitignore has an entry that makes 'git status --short' report nothing for .agents/, checked from a clean clone-equivalent by creating .agents/role.json and running git status --short.
- [x] #2 The commit does not add .agents/ itself and does not widen the ignore to .pi/, which is tracked project config.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixup applied post-review (review-pi-work over TASK-041/042/043): file was miscategorized under backlog/completed/, a non-standard folder invisible to 'backlog task list -s Done' and 'backlog task edit' (moved back to backlog/tasks/ in this fixup, same shadowing risk CLAUDE.md warns about for backlog/archive/). Neither AC checkbox had been checked and no Final Summary was ever recorded before the task was closed Done. Verified both by hand: created .agents/role.json and ran 'git status --short .agents' - empty output, confirming AC #1; diffed e4b8fee - it only adds four lines to .gitignore, never adds .agents/ itself, and does not widen the existing .pi/ralph/ entry, confirming AC #2.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The .agents/ .gitignore entry (e4b8fee) silences 'git status --short' for the loop's role.json without touching .pi/ or committing .agents/ itself. Verified directly rather than trusting the ticket record, which had been closed with both ACs unchecked.
<!-- SECTION:FINAL_SUMMARY:END -->
