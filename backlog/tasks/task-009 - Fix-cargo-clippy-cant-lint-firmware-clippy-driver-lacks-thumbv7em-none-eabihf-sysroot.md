---
id: TASK-009
title: >-
  Fix: cargo clippy can't lint firmware/ (clippy-driver lacks
  thumbv7em-none-eabihf sysroot)
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 14:31'
updated_date: '2026-08-01 14:31'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust+embedded firmware workspace (firmware/, cross-compiled to thumbv7em-none-eabihf) plus a host Cargo workspace (crates/asperitas-dsp, crates/asperitas-cli) at the repo root. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions (daisy-embassy's rev, embassy-* versions, etc.) — only the flake.nix toolchain wiring is in scope.

1. Open flake.nix at the repo root. Find the `rustWithTarget = f.combine [ ... ]` binding (introduced in TASK-004.01) and the `devShells.${system}.default = pkgs.mkShell { packages = [ ... ] }` list below it, which currently lists `rustWithTarget` alongside separate `f.stable.clippy`, `f.stable.rustfmt`, `f.stable.rust-analyzer`, `f.stable.rust-src` entries.
2. Change the combine call to also include `f.stable.clippy` (and keep the other components already combined: `f.stable.rustc`, `f.targets.thumbv7em-none-eabihf.stable.rust-std`, `f.stable.llvm-tools`), so clippy-driver shares the same merged sysroot as rustc. Remove the now-redundant standalone `f.stable.clippy` entry from the packages list (it would otherwise shadow or conflict with the combined one). Leave `f.stable.cargo`, `f.stable.rustfmt`, `f.stable.rust-analyzer`, and `f.stable.rust-src` as separate, uncombined packages — only rustc/clippy/the target std/llvm-tools need to share a sysroot; cargo and the other dev tools don't compile code directly against a target sysroot.
3. Rename the combined binding if helpful for clarity (e.g. `rustWithTarget` -> `rustToolchain`), updating its one use site in `packages`.
4. Run `nix flake check` at the repo root to confirm the flake still evaluates.
5. Enter the shell fresh (`nix develop -c bash -c 'rustc --print sysroot; clippy-driver --print sysroot'`) and confirm both print the identical path.
6. From `firmware/`, run `nix develop <repo-root> -c cargo clippy --release --features seed3 --bin blinky -- -D warnings` and confirm it runs a real lint pass (fix any lints it surfaces on firmware/src/bin/blinky.rs and firmware/src/bin/main.rs — keep fixes minimal and mechanical; if a lint requires a real design call, leave it and note it in the implementation notes instead of guessing).
7. From the repo root, run `nix develop -c cargo clippy --all-targets -- -D warnings` and confirm the host workspace (asperitas-dsp, asperitas-cli) is still clean.
8. From `firmware/`, run `nix develop <repo-root> -c cargo build --release --features seed3 --bin blinky` and confirm it still succeeds and produces a binary via `cargo objcopy` (no regression from the toolchain rewire).
9. Commit flake.nix and flake.lock (if `nix flake check`/`nix develop` regenerated the lock) together.
<!-- SECTION:PLAN:END -->
