---
id: TASK-010
title: >-
  Fix: extract duplicated defmt logger/panic boilerplate shared by
  firmware/src/bin/*.rs
status: To Do
assignee: []
created_date: '2026-08-01 14:31'
labels:
  - review-followup
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
- [ ] #3 nix develop -c cargo build --release --features seed3 --bin blinky (from firmware/) still produces a working binary of comparable size to before (~17-18 KB)
- [ ] #4 nix develop -c cargo build --release --features seed3 --bin main (from firmware/) still compiles cleanly
- [ ] #5 nix develop -c cargo clippy --all-targets -- -D warnings passes for the host workspace
<!-- AC:END -->
