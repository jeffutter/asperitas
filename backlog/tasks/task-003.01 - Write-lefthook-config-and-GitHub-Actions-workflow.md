---
id: TASK-003.01
title: Write lefthook config and GitHub Actions workflow
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:56'
updated_date: '2026-08-01 16:31'
labels: []
dependencies: []
parent_task_id: TASK-003
type: chore
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author the hook and CI configuration. No remote exists yet, so the Actions workflow cannot be exercised here — that is TASK-003.02.

pre-commit (fast): `cargo fmt --check`, clippy on changed crates.
pre-push (full, BOTH workspaces): fmt check, `clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release --features seed3` in firmware/. The cross-compile check is the important one — without it the firmware silently rots while day-to-day work happens on host crates.

Install hooks from the flake shellHook so a fresh clone gets them with no manual step. Use the nix flake in CI so local and CI toolchains are identical.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `lefthook install` runs automatically on entering the nix shell
- [ ] #2 pre-commit rejects a commit containing misformatted Rust
- [ ] #3 pre-push rejects a push whose firmware workspace does not cross-compile
- [ ] #4 A GitHub Actions workflow exists running fmt, clippy, test, and the thumbv7em cross-compile, via the same nix flake
<!-- AC:END -->
