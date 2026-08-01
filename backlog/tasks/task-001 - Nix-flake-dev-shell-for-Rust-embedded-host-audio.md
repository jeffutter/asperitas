---
id: TASK-001
title: Nix flake dev shell for Rust embedded + host audio
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:44'
updated_date: '2026-08-01 06:08'
labels: []
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
- [ ] #6 `yq` and `jq` are available in the shell (required by .pi/extensions/ralph/unblocked-todo.sh; currently satisfied only by the ambient system)
<!-- AC:END -->
