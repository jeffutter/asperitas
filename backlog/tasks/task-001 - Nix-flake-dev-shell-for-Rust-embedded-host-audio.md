---
id: TASK-001
title: Nix flake dev shell for Rust embedded + host audio
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:44'
updated_date: '2026-08-01 12:40'
labels:
  - planned
dependencies: []
documentation:
  - docs/reference/rust-daisy-stack.md
priority: high
type: chore
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the placeholder flake with a real dev shell providing every dependency this project needs, so that `direnv allow` is the only setup step.

nixpkgs' stock `rustc` does not ship the `thumbv7em-none-eabihf` target, so the toolchain must come from `fenix` or `rust-overlay`.

Needs to cover three distinct dependency groups:
- **Rust toolchain**: stable rustc/cargo with the `thumbv7em-none-eabihf` target, plus clippy, rustfmt, rust-analyzer, and llvm-tools (for `cargo-binutils`).
- **Embedded tooling**: `dfu-util` (probe-free flashing over the Seed3's USB-C), `probe-rs` (for when a debug probe arrives), `cargo-binutils` (objcopy to raw .bin for DFU).
- **Host audio**: ALSA and pkg-config so `cpal` builds, since the TUI and CLI are host binaries.

Also provide `lefthook` for the hooks in task 3.

See docs/reference/rust-daisy-stack.md for the toolchain section.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `nix develop` (or `direnv allow`) succeeds from a clean checkout
- [ ] #2 `rustc --print target-list | grep thumbv7em-none-eabihf` matches, and `rustup target list --installed`-equivalent shows the target available to cargo
- [ ] #3 `dfu-util --version`, `probe-rs --version`, `lefthook version`, and `cargo objcopy --version` all run inside the shell
- [ ] #4 pkg-config finds ALSA, verified by a trivial crate depending on `cpal` compiling
- [ ] #5 flake.lock is committed and `nix flake check` passes
- [ ] #6 `yq` (mikefarah/yq-go, not the Python yq) and `jq` remain available — required by backlog/unblocked-todo.sh, which both Ralph loops call to resolve ticket dependencies
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Replace the placeholder flake.nix with a production dev shell using fenix for the Rust toolchain.

## File changes

### flake.nix — complete rewrite

1. **Add fenix input** — `inputs.fenix.url = "github:nix-community/fenix";` alongside existing nixpkgs input
2. **Define system variable** — keep `x86_64-linux` (project is Linux-only per AC; no macOS support needed yet)
3. **Build Fenix toolchain** using `fenix.combine` with:
   - Components: `[ fenix.stable.cargo fenix.stable.rustc fenix.stable.clippy fenix.stable.rustfmt fenix.stable.rust-analyzer fenix.stable.rust-src ]`
   - Targets: `[ fenix.targets.thumbv7em-none-eabihf.stable.rust-std ]`
   - Extensions: `fenix.stable.llvm-tools` (NOT `llvm-tools-preview` — renamed in Rust 1.67+)
4. **devShell packages** — combine toolchain + nixpkgs packages:
   - From fenix toolchain: rustc, cargo, clippy, rustfmt, rust-analyzer, llvm-tools (objcopy)
   - Embedded: `pkgs.dfu-util`, `pkgs.probe-rs-tools` (NOT probe-rs — renamed in nixpkgs), `pkgs.cargo-binutils`
   - Host audio: `pkgs.alsa-lib`, `pkgs.pkg-config`
   - Tooling: `pkgs.lefthook`, `pkgs.yq-go` (NOT yq — different package), `pkgs.jq`
5. **flake.lock** — run `nix flake update` after writing flake.nix to pin all inputs

## Verification approach (agent-executable subset)

- AC #1 (nix develop succeeds): verify by building the devShell derivation
- AC #2 (target available): check that the combined toolchain includes thumbv7em-none-eabihf stdlib
- AC #3 (tools available): verify each package resolves in the shell expression
- AC #4 (ALSA found by pkg-config): requires a trivial cpal crate — defer to TASK-002 when Cargo workspace exists. The flake correctly provides alsa-lib + pkg-config which sets PKG_CONFIG_PATH automatically.
- AC #5 (flake.lock committed, nix flake check passes): commit updated lock file
- AC #6 (yq-go + jq remain): explicit inclusion in packages list

## Implementation order

1. Write new flake.nix with fenix overlay and complete devShell
2. Run `nix flake update` to regenerate flake.lock
3. Verify derivation builds: `nix build .#devShells.x86_64-linux.default` or `nix develop --print-build-logs --command true`
4. Commit both files
<!-- SECTION:PLAN:END -->
