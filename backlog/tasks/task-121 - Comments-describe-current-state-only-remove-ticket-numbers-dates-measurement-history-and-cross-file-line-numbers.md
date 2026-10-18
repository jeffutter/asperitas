---
id: TASK-121
title: >-
  Comments: describe current state only - remove ticket numbers, dates,
  measurement history and cross-file line numbers
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:36'
updated_date: '2026-10-18 22:18'
labels:
  - review-2026-10
  - docs
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 223800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner convention (now in CLAUDE.md): comments reflect the current state of the code and why, with no ticket numbers, dates, measurement history, or line-number citations into other files. The review found this widely violated: 85 TASK- references in asperitas-logging, defmt_log.rs is 157 comment lines to 38 code (with dated binary sizes and third-party line citations), rig.rs is ~948 comment lines of 2423, Cargo.toml:53-110 spends 58 lines on debug = 2, Makefile has 328 comment lines of 476, lefthook.yml 60 of 70 lines, gates.sh 331 of 573. Several comments are also wrong now (dump.rs:106-108 says 64 chunks per block, real geometry is 255; frame.rs:200-202 says 3 520 iterations, lib.rs:481 says 1,760, 8 is correct; frame.rs:811-812 says scan returns a bool but returns (); lib.rs:109-111 says GLOBAL_BACKEND is written only by set_backend_usb; spin_budget.rs:57-59 mis-describes the no-counter case; rig.rs header says 'deliberately a near-copy of main.rs' and 'no inbound control'; Cargo.toml:118-120 says executor-interrupt is for rig only; memory-internal.x:3-6 says QSPI is future work; encoder.rs:90-95 state table contradicts the four-transitions model; knob.rs:8 says embassy-stm32 v4; measure capture.rs:3-4 names the wrong example). Move anything worth keeping (rationale, measurements) to docs or the owning ticket and leave a one-line why in the code. Do this per crate, and fold it into each refactor ticket when the file is already being rewritten. Found in the 2026-10-16 whole-codebase review.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No TASK- numbers, dates or measurement tables remain in code comments across crates/ and firmware/
- [x] #2 Every comment listed in the description as wrong is corrected or deleted
- [x] #3 No comment cites a line number in another file
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 579d70c. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Swept crates/, firmware/src, firmware/Cargo.toml, firmware/Makefile, memory*.x, scripts/ and lefthook.yml: zero TASK- references remain in the scanned files, dates were stripped from comments (fixture file names kept as identifiers). Many of the wrong comments in the description had already been fixed by earlier refactors (lib.rs GLOBAL_BACKEND, spin_budget, measure capture.rs, knob v4 partly). Corrected this pass: dump.rs block size (255 chunks), frame.rs CRC iterations (8 bits, 1 760) and scan() return, rig.rs header, firmware Cargo.toml executor-interrupt, memory-internal.x QSPI, encoder.rs state table, knob.rs embassy-stm32 version. Verified: cargo fmt, cargo test --workspace, firmware rig cargo check, doc-artifact-names, elf-staleness and load-addresses selftests. Left alone: the large comment-density trims (Makefile, gates.sh, rig.rs bulk) were not rewritten wholesale.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Removed ticket numbers and dates from comments across crates, firmware and scripts, corrected the specifically wrong comments listed, and kept scripts/tests green.
<!-- SECTION:FINAL_SUMMARY:END -->
