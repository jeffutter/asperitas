---
id: TASK-044
title: >-
  Run cargo fmt inside firmware/ to clear pre-existing drift in main.rs and
  podtest.rs
status: Done
assignee:
  - '@agent'
created_date: '2026-09-10 06:30'
updated_date: '2026-09-10 21:45'
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
- [x] #1 cd firmware && cargo fmt --all makes the tree clean: cd firmware && cargo fmt --all --check exits 0 (currently fails on Ticker::every wrapping in src/bin/main.rs:148 and src/bin/podtest.rs:216).
- [x] #2 cd firmware && cargo build --release --features seed3 still succeeds and objdump -f shows a 0x0800 start address with .vector_table at 08000000.
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 21:45
---
Fixup applied post-review (review-pi-work over TASK-041/042/043): file was filed under backlog/completed/, a non-standard folder invisible to 'backlog task list -s Done' and 'backlog task edit' - the same shadowing risk CLAUDE.md warns about for backlog/archive/. Content (ACs, Final Summary) was already correct; moved back to backlog/tasks/ so the tool can see it.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Ran `cargo fmt --all` in firmware/. Two call sites re-wrapped (embassy_time::Ticker::every in src/bin/main.rs, src/bin/podtest.rs); pure whitespace, confirmed with `git diff -w` showing no syntactic change.

Verified:
- `cargo fmt --all --check` exits 0
- `cargo build --release --features seed3` succeeds
- rust-objdump -f on both main and podtest: start address 0x08000299, .vector_table at 08000000

Commit f2719d1 style(firmware): cargo fmt Ticker::every call sites
<!-- SECTION:FINAL_SUMMARY:END -->
