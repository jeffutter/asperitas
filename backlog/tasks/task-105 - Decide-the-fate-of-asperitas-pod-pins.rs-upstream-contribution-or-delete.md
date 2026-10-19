---
id: TASK-105
title: Decide the fate of asperitas-pod pins.rs (upstream contribution or delete)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 02:22'
labels:
  - review-2026-10
  - pod
  - planned
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
- [x] #1 Decision recorded and pins.rs deleted or relocated; daisy-pod.md matches
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 9b041d8
Owner decision (2026-10-16): delete pins.rs.
1. git rm crates/asperitas-pod/src/pins.rs; remove the cfg(feature = pod-hw) 'pub mod pins;' from crates/asperitas-pod/src/lib.rs.
2. Fix lib.rs module docs (lines 3-5): drop 'pin map type aliases and structs' and the 'contributed upstream unchanged' claim; describe the crate as control drivers (knob, encoder, led).
3. docs/reference/daisy-pod.md section 'No Pod support exists in daisy-embassy' (~line 156): state that no Pod pin map module exists in daisy-embassy or here, firmware takes board pins directly, and no upstream contribution is planned. Remove 'good candidate to contribute upstream'.
4. grep for stray refs (PodPins, pins.rs) outside backlog/.
5. Verify: cargo check and cargo test -p asperitas-pod on host, plus the pod-hw firmware build used by the repo.
6. Tick AC #1 and record the decision in notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Owner has no use for pins.rs (it was a possible upstream daisy-embassy contribution). Delete it and fix daisy-pod.md.

Deleted pins.rs per owner decision; removed the pins module, fixed lib.rs docs and daisy-pod.md. Verified: cargo test -p asperitas-pod, firmware cargo check --release.
<!-- SECTION:NOTES:END -->
