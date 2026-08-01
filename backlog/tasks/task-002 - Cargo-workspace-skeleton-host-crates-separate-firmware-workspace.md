---
id: TASK-002
title: 'Cargo workspace skeleton: host crates + separate firmware workspace'
status: To Do
assignee: []
created_date: '2026-08-01 05:45'
labels: []
dependencies:
  - TASK-001
documentation:
  - docs/reference/rust-daisy-stack.md
priority: high
type: feature
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the crate structure the rest of the project hangs off. No real logic yet — the deliverable is that both targets build.

**Two workspaces, deliberately.** A single workspace cannot cleanly hold both host and `thumbv7em-none-eabihf` crates: `forced-target` is unstable, and a workspace-wide `[build] target` breaks the host crates. So `firmware/` is excluded from the root workspace and carries its own `.cargo/config.toml`.

Layout:
```
crates/asperitas-dsp/     # no_std core, no hardware deps
crates/asperitas-cli/     # host
firmware/                 # separate workspace, thumbv7em-none-eabihf
```

`asperitas-tui` and `asperitas-bench` come later; don't create empty shells for them now.

`asperitas-dsp` is `#![no_std]` with a `std` feature enabled by dev-dependencies for tests. It must not depend on embassy, daisy-embassy, or anything hardware-facing — that boundary is the whole architectural point.

For firmware, pin daisy-embassy to PR #80's **commit SHA**, not the branch:
```toml
daisy-embassy = { git = "https://github.com/landakram/daisy-embassy", rev = "477083b0227d" }
```
The PR is days old and unreviewed; it may be force-pushed. See docs/reference/rust-daisy-stack.md.

Firmware `.cargo/config.toml` should keep daisy-embassy's linker flags but NOT assume probe-rs as the runner, since there is no debug probe yet (task 4 covers flashing).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `cargo build` at the repo root builds all host crates
- [ ] #2 `cargo build --release --features seed3` inside firmware/ cross-compiles to thumbv7em-none-eabihf
- [ ] #3 asperitas-dsp is `#![no_std]` and `cargo tree -p asperitas-dsp` shows no embassy/daisy/hardware dependency
- [ ] #4 asperitas-dsp's tests run on host via a `std` feature
- [ ] #5 daisy-embassy is pinned by `rev` to a specific SHA, not by branch name
<!-- AC:END -->
