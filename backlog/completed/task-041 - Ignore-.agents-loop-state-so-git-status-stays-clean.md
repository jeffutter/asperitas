---
id: TASK-041
title: Ignore .agents/ loop state so git status stays clean
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 19:43'
updated_date: '2026-09-10 05:03'
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
- [ ] #1 .gitignore has an entry that makes 'git status --short' report nothing for .agents/, checked from a clean clone-equivalent by creating .agents/role.json and running git status --short.
- [ ] #2 The commit does not add .agents/ itself and does not widen the ignore to .pi/, which is tracked project config.
<!-- AC:END -->
