---
id: TASK-105
title: Decide the fate of asperitas-pod pins.rs (upstream contribution or delete)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - pod
dependencies: []
priority: low
type: chore
ordinal: 207800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
asperitas-pod/src/pins.rs (PodPins and its 10 aliases) is referenced by nothing. daisy-pod.md:156-161 even says no Pod pin map exists. It was written for a possible upstream daisy-embassy contribution.

DECISION NEEDED: is that contribution still planned? If not, delete; if so, move it out of this tree until it has a user.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Decision recorded and pins.rs deleted or relocated; daisy-pod.md matches
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Owner has no use for pins.rs (it was a possible upstream daisy-embassy contribution). Delete it and fix daisy-pod.md.
<!-- SECTION:NOTES:END -->
