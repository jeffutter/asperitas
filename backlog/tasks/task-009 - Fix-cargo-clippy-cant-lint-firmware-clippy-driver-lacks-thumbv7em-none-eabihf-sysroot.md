---
id: TASK-009
title: >-
  Fix: cargo clippy can't lint firmware/ (clippy-driver lacks
  thumbv7em-none-eabihf sysroot)
status: To Do
assignee: []
created_date: '2026-08-01 14:31'
labels:
  - review-followup
dependencies:
  - TASK-004.01
documentation:
  - docs/reference/rust-daisy-stack.md
priority: high
type: bug
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-004.01 (flake.nix). TASK-004.01 fixed `rustc`'s sysroot by combining it with the thumbv7em-none-eabihf target std via `f.combine [f.stable.rustc f.targets.thumbv7em-none-eabihf.stable.rust-std f.stable.llvm-tools]` (see flake.nix devShell packages), so `cargo build`/`cargo objcopy` work for firmware/. But `f.stable.clippy` was left out of that combine and is still listed as a separate top-level package. `clippy-driver` therefore resolves to its own standalone sysroot (verified: `rustc --print sysroot` and `clippy-driver --print sysroot` point at two different Nix store paths in the dev shell), which has no thumbv7em-none-eabihf std. Running `cargo clippy --release --features seed3 --bin blinky` inside firmware/ fails with `error[E0463]: can't find crate for \`core\`` before clippy ever inspects the code. This is a Resilient/Organized-axis gap: firmware Rust code has been completely unlintable since TASK-004.01 landed, and nobody will notice until they try it and hit a confusing sysroot error rather than a lint result. TASK-005.01 (audio passthrough) is about to add real firmware logic that should be clippy-checked.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `clippy-driver --print sysroot` and `rustc --print sysroot` resolve to the same combined toolchain inside `nix develop`
- [ ] #2 `cd firmware && nix develop -c cargo clippy --release --features seed3 --bin blinky -- -D warnings` succeeds and actually lints the code (not an E0463 sysroot error)
- [ ] #3 `nix develop -c cargo clippy --all-targets -- -D warnings` (host workspace, from repo root) still passes clean, confirming host clippy isn't broken by the change
- [ ] #4 nix develop -c cargo build --release --features seed3 --bin blinky (firmware/) still produces a working binary — no build regression from the toolchain change
<!-- AC:END -->
