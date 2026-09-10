---
id: TASK-044
title: >-
  Run cargo fmt inside firmware/ to clear pre-existing drift in main.rs and
  podtest.rs
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 06:30'
labels: []
dependencies: []
priority: medium
type: task
ordinal: 73500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Discovered while executing TASK-030.05: 'cargo fmt --all --check' inside firmware/ reports diffs in src/bin/main.rs and src/bin/podtest.rs (both reformat embassy_time::Ticker::every call sites). Verified pre-existing: identical two diffs at HEAD c023286 in a clean worktree without any TASK-030.05 changes. Not gated by CI (ci.yml only builds firmware) nor lefthook, so it drifted silently. TASK-030.05 could not fix these because its AC #2 forbids any non-comment change under firmware/. Pure whitespace commit; no behavior change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cd firmware && cargo fmt --all makes the tree clean: cd firmware && cargo fmt --all --check exits 0 (currently fails on Ticker::every wrapping in src/bin/main.rs:148 and src/bin/podtest.rs:216).
- [ ] #2 cd firmware && cargo build --release --features seed3 still succeeds and objdump -f shows a 0x0800 start address with .vector_table at 08000000.
<!-- AC:END -->
