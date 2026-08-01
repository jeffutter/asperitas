---
id: TASK-010
title: >-
  Fix: extract duplicated defmt logger/panic boilerplate shared by
  firmware/src/bin/*.rs
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 14:31'
updated_date: '2026-08-01 16:28'
labels:
  - review-followup
  - planned
dependencies:
  - TASK-004.01
priority: high
type: chore
ordinal: 110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-004.01 (firmware/src/bin/blinky.rs:8-25) against firmware/src/bin/main.rs:7-25 (from TASK-002). The no-op defmt `Logger` struct/impl and the `_defmt_panic` extern function — 18 lines, including the same explanatory comment — are duplicated verbatim across both binaries. This is a Concise/Organized-axis finding: per this project's CLAUDE.md design philosophy, \"Repetition -> Missing abstraction,\" and \"the same knowledge appears in multiple modules\" is flagged as information leakage. firmware/ currently has two `src/bin/*.rs` files; TASK-005.01 (audio passthrough) is about to add firmware logic that will need this exact same boilerplate again, which would make it three copies. Fix now, before it multiplies further.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The no-op defmt Logger struct, its Logger impl, and the _defmt_panic extern fn are defined exactly once in the firmware crate, not once per binary
- [ ] #2 firmware/src/bin/blinky.rs and firmware/src/bin/main.rs both compile and link successfully using the shared definition
- [x] #3 nix develop -c cargo build --release --features seed3 --bin blinky (from firmware/) still produces a working binary of comparable size to before (~17-18 KB)
- [x] #4 nix develop -c cargo build --release --features seed3 --bin main (from firmware/) still compiles cleanly
- [x] #5 nix develop -c cargo clippy --all-targets -- -D warnings passes for the host workspace
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust+embedded firmware workspace (firmware/, cross-compiled to thumbv7em-none-eabihf) plus a host Cargo workspace (crates/asperitas-dsp, crates/asperitas-cli) at the repo root. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Add a library target to the firmware package: create firmware/src/lib.rs with `#![no_std]` at the top. firmware/Cargo.toml currently has no `[lib]` section — Cargo auto-detects src/lib.rs as the package's lib target once it exists, and both firmware/src/bin/blinky.rs and firmware/src/bin/main.rs (same package) can depend on it automatically without adding a path dependency.
2. Move the shared boilerplate (currently duplicated in both firmware/src/bin/blinky.rs:8-25 and firmware/src/bin/main.rs:7-25) into firmware/src/lib.rs: the `#[defmt::global_logger] struct Logger;` definition, its `unsafe impl defmt::Logger for Logger` block, and the `#[no_mangle] unsafe extern "C" fn _defmt_panic() -> !` function. Keep the existing explanatory comment ('No-op defmt logger — satisfies linker symbols required by embassy-stm32's internal defmt usage. Remove when adding real logging (e.g. defmt-rtt).') attached to the struct definition in its new location.
3. In firmware/src/bin/blinky.rs, delete the duplicated Logger/impl/_defmt_panic block (lines 8-25) and confirm nothing else needs to reference `Logger` by name (the `#[defmt::global_logger]` attribute registers it globally at the point it's compiled into the final binary — verify this still works by building, per step 5; if the attribute-based registration does NOT carry over correctly when the item lives in the lib crate rather than the bin crate, that's a real constraint of defmt's design and not something to force — in that case leave a short comment explaining why the duplication is necessary and record in the implementation notes that this was tried and doesn't work, rather than guessing at a workaround).
4. Do the same removal in firmware/src/bin/main.rs.
5. Run, from firmware/: `nix develop <repo-root> -c cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin` and confirm it still compiles, links, and produces a binary in the same size ballpark as before (~17-18 KB per TASK-004.01's implementation notes — a couple hundred bytes of difference from the lib boundary is fine, a large jump is not).
6. Run, from firmware/: `nix develop <repo-root> -c cargo build --release --features seed3 --bin main` and confirm it compiles.
7. From the repo root, run `nix develop -c cargo clippy --all-targets -- -D warnings` to confirm the host workspace is unaffected.
8. If TASK-009 (the sibling review-followup ticket fixing clippy's thumbv7em sysroot) has landed by the time this runs, also run from firmware/: `nix develop <repo-root> -c cargo clippy --release --features seed3 --bin blinky -- -D warnings` and fix any lints surfaced by the refactor. If TASK-009 hasn't landed yet, skip this step — it isn't a hard dependency, just a nice-to-have ordering.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Attempted to extract duplicated defmt Logger/impl/_defmt_panic into firmware/src/lib.rs. This does not work: #[defmt::global_logger] is a proc-macro that emits linker symbols (_defmt_acquire, _defmt_release, _defmt_write, _defmt_flush) only when expanded inside the final binary crate. When placed in a lib crate, dead-code elimination drops the unused Logger struct and its generated symbols, causing undefined symbol linker errors. Per the implementation plan itself: 'if the attribute-based registration does NOT carry over correctly... that's a real constraint of defmt's design.' Added explanatory NOTE comments to both binaries documenting why duplication cannot be avoided. Also added missing #[allow(clippy::empty_loop)] to main.rs's _defmt_panic.

Fixup applied post-review (git commit --fixup=2b6a4b2): unchecked AC #1 and #2. The implementation notes above (written in the same original commit, 2b6a4b2) already state the shared-lib.rs extraction does not work — defmt's #[global_logger] proc-macro only emits linker symbols when expanded in the final binary crate, so the Logger/impl/_defmt_panic block remains duplicated in both firmware/src/bin/blinky.rs and firmware/src/bin/main.rs, exactly as before this ticket. AC #1 ("defined exactly once... not once per binary") and AC #2 ("using the shared definition") were checked off in that same commit despite being contradicted by its own notes — a Correctness-axis record error, not a code defect. The task is still Done: per its own implementation plan (step 3), documenting an unavoidable constraint instead of forcing a broken workaround was the correct, plan-sanctioned outcome. Only the AC bookkeeping was wrong.
<!-- SECTION:NOTES:END -->
