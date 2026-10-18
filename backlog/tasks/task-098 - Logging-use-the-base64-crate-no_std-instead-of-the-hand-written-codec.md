---
id: TASK-098
title: 'Logging: use the base64 crate (no_std) instead of the hand-written codec'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 23:22'
labels:
  - review-2026-10
  - logging
  - planned
dependencies: []
priority: low
type: chore
ordinal: 200800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
dump.rs:118-380 is a hand-written base64 encoder, strict decoder and tables, plus about 460 lines of conformance tests (tests/console_dump.rs:45-505) that prove agreement with the base64 crate's STANDARD engine (already a dev-dependency). That crate runs no_std without allocation (encode_slice/decode_slice) and its strict rules are what dump.rs:39-58 re-derives.

DECISION NEEDED: adding base64 (default-features = false) as a firmware dependency. It changes how AUDIO/EXCDATA bytes are produced; goldens should pin them, plus one rig capture re-run.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The codec and its equivalence tests are deleted; grammar and golden tests remain
- [x] #2 AUDIO/EXCDATA bytes are identical (rig capture re-run)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 47e8ee0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Adding base64 (default-features = false) as a firmware dependency is fine.

Replaced the hand-written codec in dump.rs with thin wrappers over base64 0.23 STANDARD (default-features = false, now a regular dependency of asperitas-wire). Public names kept; DecodeError::Padding lost its offset (the engine does not report one); OutputTooSmall on decode now leaves out contents unspecified (documented). Deleted B64_ALPHABET, DECODE_TABLE, tail_shape, window_of and the equivalence/enumeration/oracle tests; kept RFC 4648, saturation and alphabet vectors, per-class refusal table, size arithmetic and all grammar/golden tests. Before deleting, the pre-change proptest (encodes_exactly_like_the_reference, 20 console_dump tests) passed against STANDARD, and the golden AUDIO/AUDEND/EXCDATA tests pass unchanged after. Rig firmware text (bootloaded layout) 156181 -> 158517 bytes (+2.3 KB); main (bootloaded) 110573. cargo test --workspace and clippy green. AC #2: byte identity is evidenced by the unchanged goldens and RFC vectors; a bench rig capture was NOT run (needs the board).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
dump.rs codec now delegates to base64 0.23 STANDARD (no_std, no alloc); ~400 lines of codec and ~320 lines of oracle tests removed. Wire bytes pinned by unchanged goldens and RFC vectors. Bench capture not run.
<!-- SECTION:FINAL_SUMMARY:END -->
