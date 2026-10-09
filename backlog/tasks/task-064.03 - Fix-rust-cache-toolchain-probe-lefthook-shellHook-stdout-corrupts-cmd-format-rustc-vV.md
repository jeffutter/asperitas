---
id: TASK-064.03
title: >-
  Fix rust-cache toolchain probe: lefthook shellHook stdout corrupts cmd-format
  rustc -vV
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 13:27'
labels:
  - ci
dependencies: []
references:
  - .github/workflows/ci.yml
  - flake.nix
parent_task_id: TASK-064
ordinal: 148800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Run 37925833149 (first run after TASK-064.01) shows the cache never engaged. The rust-cache step logged '##[error]Command failed: nix develop .#default --command rustup run sync hooks: (pre-commit, pre-push) rustc -vV' then 'error: toolchain sync is not installed'. Cause: flake.nix shellHook runs 'lefthook install', which prints 'sync hooks: ...' on stdout; rust-cache's cmd-format wraps 'rustc -vV' in 'nix develop --command', so that line is prepended to the output and the action falls through to 'rustup run <first token>'. No Cache Key, no 'Restored from cache', and the Post step produced no save. Fix: make the shell entry silent on stdout (e.g. 'lefthook install >&2' or quiet flag) so cmd-format output is clean, then confirm on a runner that the rust-cache log prints Cache Key and Rust Versions with the nix rustc. Also note the 'Toolchain identity' step shows rustup at /home/runner/.cargo/bin/rustup reachable inside the nix shell (rustc itself resolves to /nix/store), and that step took 10m41s building the toolchain with no nix store cache - worth a decision whether to cache the nix store too.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 flake.nix shellHook emits nothing on stdout, so 'nix develop .#default --command rustc -vV' prints only rustc's output (check locally)
- [ ] #2 scripts or a selftest guards that 'nix develop .#default --command true' stdout is empty
- [ ] #3 HUMAN-free: a pushed CI run's rust-cache step log shows a Cache Key line and Rust Versions listing the nix-provided rustc, with no ##[error] annotation
<!-- AC:END -->
