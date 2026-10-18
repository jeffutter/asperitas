---
id: TASK-096
title: >-
  Logging: one body writer/reader, one saturating counter, one field list per
  verb
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 23:07'
labels:
  - review-2026-10
  - logging
  - planned
dependencies: []
priority: low
type: chore
ordinal: 198800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three ways to write record bodies: TruncWriter + write! (console verbs), dump::put/put_hex/put_decimal (dump.rs:571-590, also used by excerpt and install), control::Writer/Reader (control.rs:536-597) which re-implement frame::write_hex/parse_hex. Saturating-counter helpers are copied in console.rs:161-170, inbound.rs:166, control.rs:451. Each console verb lists its fields twice, in the format string and in a *_FIELDS table (console.rs:311-347), kept in sync only by runtime tests. Test helpers are duplicated too: park_cursors_at (frame.rs:1579, tests/console_dump.rs:1967), decode-in-chunks helpers, and three hand-rolled xorshift RNGs although proptest is a dependency.

Change: one body module with a bounded writer (field_u32, hex) and reader, worst-case lengths derived from one field list, one SaturatingCounter newtype, a shared tests/common/mod.rs and proptest strategies.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 One body writer/reader and one saturating counter
- [x] #2 Wire bytes unchanged (golden tests pass)
- [x] #3 Test helpers and RNGs deduplicated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 34894e2 (with 521ed12). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in two commits. 521ed12: SaturatingCounter (asperitas-wire counter module) replaces the three copies in console, control and inbound; the console add() used checked_add and stalled below u32::MAX on overflow, now lands exactly on the ceiling. body module: BodyWriter (non-panicking, supersedes TruncWriter, adds put/hex/decimal/rest_mut/advance) and BodyReader (moved from dump, adds labelled_hex) replace control Writer/Reader and dump put/put_hex/put_decimal. 34894e2: wire_verb! macro makes each console verb's field list the only place its fields are spelled; it renders the body and derives the const worst-case length from per-kind widths, replacing the *_FIELDS tables. Shared tests/common/mod.rs (park_cursors_at, seeded proptest TestRng, decode_in_chunks) is included by #[path] into wire unit tests, wire integration tests and logging tests; the three hand-rolled RNGs are gone. Golden and wire-byte tests pass unchanged; cargo test/clippy workspace clean and commit-tier gates green. Left alone on purpose: the DescWriter copy in asperitas-dsp (leaf crate), and console_frame's Outcome-based decode helper (richer law-checking variant, not a duplicate).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
One SaturatingCounter, one BodyWriter/BodyReader, console verb field lists as single source (wire_verb! macro), shared test helpers and proptest RNG. Wire bytes unchanged; goldens pass.
<!-- SECTION:FINAL_SUMMARY:END -->
