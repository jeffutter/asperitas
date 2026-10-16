---
id: TASK-118
title: >-
  Docs: refresh rust-daisy-stack.md, mark idea.md superseded, fix daisy-pod.md,
  move logs out of reference/
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - docs
dependencies: []
priority: low
type: chore
ordinal: 220800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rust-daisy-stack.md is a 2026-08-01 snapshot patched piecemeal: '0.2.2 is the last published' (:23; firmware/Cargo.lock:230 has 0.2.3), 'vendored fork' and the archived TASK-005.03 (:57), dasp as 'chosen base' (:76, unused), host-side crates cpal and ratatui (:82-87, neither a dependency), the STM32H750IBKx chip string (:103). idea.md is a pre-project handoff whose 'Key open uncertainty' (74-78) doc-001 settled and which proposes egui and probe-rs as the main loop. daisy-pod.md:156-161 says no Pod pin map exists though crates/asperitas-pod/src/pins.rs does, and :220 records a listening session inside a hardware reference. seed3-schematic-search-log.md is a research log under reference/. Some backlog filenames contain em dashes (task-006.01.05, 024, 025).

Change: date-stamp or rewrite sections as current state, give idea.md a 'superseded by doc-001' header, point daisy-pod.md at asperitas-pod, move session and search logs to docs/investigations/ (default location unless the owner prefers ticket notes only), rename the em-dash backlog files.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each listed document is accurate or clearly marked historical
- [ ] #2 No em dashes in backlog filenames
<!-- AC:END -->
