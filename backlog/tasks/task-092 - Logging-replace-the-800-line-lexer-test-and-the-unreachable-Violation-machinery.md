---
id: TASK-092
title: >-
  Logging: replace the 800-line lexer test and the unreachable Violation
  machinery
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
  - needs-decision
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
- [ ] #1 The no-panic rule is enforced by a maintained tool or lints, not a hand-written lexer
- [ ] #2 Violation machinery and its unreachable branches are deleted
- [ ] #3 Release behaviour is unchanged
<!-- AC:END -->
