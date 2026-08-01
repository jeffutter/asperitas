---
id: TASK-002
title: 'Cargo workspace skeleton: host crates + separate firmware workspace'
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 05:45'
updated_date: '2026-08-01 14:30'
labels:
  - planned
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
- [x] #1 `cargo build` at the repo root builds all host crates
- [x] #2 `cargo build --release --features seed3` inside firmware/ cross-compiles to thumbv7em-none-eabihf
- [x] #3 asperitas-dsp is `#![no_std]` and `cargo tree -p asperitas-dsp` shows no embassy/daisy/hardware dependency
- [x] #4 asperitas-dsp's tests run on host via a `std` feature
- [x] #5 daisy-embassy is pinned by `rev` to a specific SHA, not by branch name
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Overview
Create two Cargo workspaces that both build from scratch. Root workspace holds host crates (`asperitas-dsp`, `asperitas-cli`). Separate `firmware/` workspace cross-compiles for Seed3 via daisy-embassy PR #80 (SHA-pinned). No real logic — the deliverable is successful compilation.

### Files to create (11 files across 4 directories)

#### Root workspace
1. **Root `Cargo.toml`** — `[workspace]` with `members = ["crates/*"]`, `exclude = ["firmware"]`, `resolver = "2"`
2. **`crates/asperitas-dsp/Cargo.toml`** — `edition = "2021"`, `no_std`, optional `std` feature. Zero hardware deps. Dev-deps gated behind `std` feature so tests run on host without polluting the embedded target.
3. **`crates/asperitas-dsp/src/lib.rs`** — `#![no_std]` crate root with a trivial public function and a `#[cfg(feature = "std")]` test module
4. **`crates/asperitas-cli/Cargo.toml`** — `edition = "2021"`, binary crate depending on `asperitas-dsp`
5. **`crates/asperitas-cli/src/main.rs`** — `fn main() { println!("asperitas-cli"); }`

#### Firmware workspace (separate directory, excluded from root)
6. **`firmware/Cargo.toml`** — standalone `[workspace]` with `members = ["."]`. Depends on `daisy-embassy = { git = "...", rev = "477083b0227d", features = ["seed3"] }`. Also depends on `asperitas-dsp = { path = "../crates/asperitas-dsp" }` to validate the shared boundary. Features: `default = []`, `seed3` re-exported from daisy-embassy. Linker dependencies: `cortex-m-rt`, `panic-halt`.
7. **`firmware/.cargo/config.toml`** — `target = "thumbv7em-none-eabihf"`, `rustflags = ["-C", "link-arg=-Tlink.x"]`. NO runner line (no debug probe yet).
8. **`firmware/memory.x`** — STM32H750IBKx memory map: FLASH 2 MB at 0x08000000, RAM 1 MB at 0x24000000
9. **`firmware/build.rs`** — copies `memory.x` into `OUT_DIR` so linker finds it (standard embassy pattern)
10. **`firmware/src/bin/main.rs`** — minimal `#[entry] fn main() -> ! { loop {} }` with daisy-embassy initialization shell. Uses `cortex_m_rt::entry` and `embassy_executor::main` attribute if available from seed3 feature.

### Implementation order

1. Create root `Cargo.toml` + `crates/asperitas-dsp/` (Cargo.toml + lib.rs)
2. Verify `cargo build` succeeds for host crates
3. Create `crates/asperitas-cli/` (Cargo.toml + main.rs)
4. Verify `cargo build` still succeeds for all host crates
5. Create `firmware/` workspace (Cargo.toml + .cargo/config.toml + memory.x + build.rs + src/bin/main.rs)
6. Verify `cd firmware && cargo build --features seed3` cross-compiles
7. Run `cargo tree -p asperitas-dsp` to confirm no embassy/daisy/hardware leakage

### Key decisions

- **No sub-tickets**: All files are minimal stubs that must exist simultaneously for either workspace to build. Splitting would create partial states that cannot be verified independently.
- **Firmware depends on asperitas-dsp**: Even though there is no DSP code yet, wiring the dependency validates the architectural boundary (no_std crate usable from embedded context) and prevents future accidental decoupling.
- **No `runner` in firmware .cargo/config.toml**: Per ticket requirements, no debug probe assumed. Flashing via DFU is covered by TASK-004.
- **`std` feature on asperitas-dsp enables dev-dependencies**: Pattern is `dev-dependencies` with `features = ["std"]` or make test-only deps optional and gate them behind the `std` feature. Using `[features] std = []` with dev-deps that enable it keeps the embedded build clean.

### Verification commands
```bash
# Host workspace
cargo build                          # AC #1
cargo test -p asperitas-dsp --features asperitas-dsp/std   # AC #4
cargo tree -p asperitas-dsp         # AC #3

# Firmware workspace
cd firmware
cargo build --features seed3        # AC #2
grep -r "rev.*477083b0227d" Cargo.toml  # AC #5
```
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation notes:

- Root workspace (Cargo.toml): members = ["crates/*"], exclude = ["firmware"], resolver = "2"
- asperitas-dsp: #![no_std] with zero dependencies, trivial process_sample() passthrough, std feature for host tests
- asperitas-cli: binary crate depending on asperitas-dsp, minimal println main
- firmware/: separate workspace with its own .cargo/config.toml targeting thumbv7em-none-eabihf
- daisy-embassy pinned to rev 477083b0227d (PR #80, unmerged Seed3 support)
- embassy-executor uses platform-cortex-m + executor-thread features (needed for #[embassy_executor::main])
- Added defmt no-op global_logger and _defmt_panic stub because embassy-stm32 links against defmt internally
- No runner in firmware .cargo/config.toml (no debug probe yet)

Fixup applied post-review (pi review, 2026-08-01): (1) firmware/Cargo.toml pinned daisy-embassy to `daisy-embassy/daisy-embassy` instead of the ticket-specified `landakram/daisy-embassy` fork URL — corrected to match spec (both resolved the pinned SHA today via GitHub's PR-ref sharing, but only the fork URL matches what the ticket explicitly required and documented rationale exists for). (2) crates/asperitas-dsp/src/lib.rs: process_sample's parameter was named `_sample` despite being used/returned, which misleadingly reads as unused — renamed to `sample`. Both verified via `cargo build --release --features seed3` (firmware) and `cargo test -p asperitas-dsp --features asperitas-dsp/std` (host).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created two-workspace Cargo structure. Root workspace builds host crates (asperitas-dsp, asperitas-cli). Separate firmware/ workspace cross-compiles for Seed3 via daisy-embassy PR #80 (SHA-pinned). All 5 acceptance criteria verified: host build passes, firmware release build passes, dsp has zero hardware deps, tests run on host, dependency pinned by rev.
<!-- SECTION:FINAL_SUMMARY:END -->
