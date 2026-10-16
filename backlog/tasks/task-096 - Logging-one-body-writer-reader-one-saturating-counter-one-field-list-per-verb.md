---
id: TASK-096
title: >-
  Logging: one body writer/reader, one saturating counter, one field list per
  verb
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
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
- [ ] #1 One body writer/reader and one saturating counter
- [ ] #2 Wire bytes unchanged (golden tests pass)
- [ ] #3 Test helpers and RNGs deduplicated
<!-- AC:END -->
