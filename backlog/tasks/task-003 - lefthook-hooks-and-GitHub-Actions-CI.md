---
id: TASK-003
title: lefthook hooks and GitHub Actions CI
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:45'
updated_date: '2026-08-01 05:56'
labels: []
dependencies:
  - TASK-002
priority: high
type: chore
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Wire up the consistency checks. Both hooks and CI run the same set; CI exists because hooks can be bypassed with `--no-verify`.

**pre-commit** — fast enough not to be resented:
- `cargo fmt --check`
- `cargo clippy` on changed crates

**pre-push** — the full set, across BOTH workspaces:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- **`cargo build --release --features seed3` in firmware/** — this is the important one. Without a cross-compile check the firmware silently rots while all the day-to-day work happens on desktop crates.

**GitHub Actions** re-runs the pre-push set on push and PR. Needs a GitHub remote created (the repo currently has one local commit and no remote). Use the nix flake in CI so local and CI toolchains are identical.

Install the hooks from the flake's shellHook so a fresh clone gets them without a manual step.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks complete
<!-- AC:END -->
