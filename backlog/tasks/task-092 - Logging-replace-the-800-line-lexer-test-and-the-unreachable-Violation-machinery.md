---
id: TASK-092
title: >-
  Logging: replace the 800-line lexer test and the unreachable Violation
  machinery
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-17 02:23'
labels:
  - review-2026-10
  - logging
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 194800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/commit_path_no_panic.rs finds functions by strings ('fn encode(') and proves slices by needles ('v % 10', 'written += n'), so refactors break it, and by its own header (26-34) it cannot see implicit panics (indexing, overflow). It guards a branch that cannot happen: dump_fits is PREFIX+len+TRAILER <= free - RESERVE (dump.rs:539-541) and len <= MAX_BODY is refused before the lock (lib.rs:647), so write_whole's RefusedForSpace after admission is arithmetically impossible; the 'admitted' argument is always true at its only call site (lib.rs:692); the 'unreachable' counting branch at lib.rs:677-683 likewise. Violation, refusal_after_admission and its test (lib.rs:307-361, 736-779) exist for it.

Change: delete Violation/refusal_after_admission/the post-lock debug_assert; replace the lexer with dtolnay's no-panic attribute on in-lock functions (checked by the firmware release build; it catches implicit panics), or per-function deny(clippy::...).
DECISIONS NEEDED: is adding the no-panic dependency to the firmware OK, and is the debug-only Violation crash wanted at all?

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The no-panic rule is enforced by a maintained tool or lints, not a hand-written lexer
- [x] #2 Violation machinery and its unreachable branches are deleted
- [x] #3 Release behaviour is unchanged
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-092 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Adding no-panic as a firmware dependency is fine. Delete the unreachable Violation machinery and the lexer test.

Used the owner's fallback, not dtolnay no-panic. no-panic was added and the firmware release build passed, but it did not detect anything: it is a no-op on this panic=abort target (an explicit panic!() and a runtime v[idx] inside an annotated fn both linked fine). Enforcement is now #[deny(clippy::indexing_slicing, unwrap_used, expect_used, panic)] on emit_at, write_and_count, commit_dump, frame::{encode,write_whole,crc16_ccitt*,write_hex,write_decimal,sanitize_byte,level_letter}, dump::dump_fits and the four console counters. Arithmetic overflow is not linted (release wraps). Verified it bites: a runtime index in write_and_count failed cargo clippy -p asperitas-logging --features log-usb --target thumbv7em-none-eabihf; reverted. The firmware workspace does not lint these path crates, so run that command and cargo clippy -p asperitas-wire. Rewrote frame::encode/write_whole/write_hex with get()/get_mut() (same output; frame tests pass). Deleted Violation, refusal_after_admission, the post-lock debug_assert, its test and tests/commit_path_no_panic.rs. RefusedForSpace still counts dropped_full/bytes_dropped via write_and_count. cargo test --workspace passes except asperitas-measure (pre-existing, TASK-123).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Deleted the Violation machinery and the 800-line lexer test; the in-lock path now has shared write_and_count and commit_dump functions guarded by clippy deny lints (no-panic proved ineffective on panic=abort). Release behaviour unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
