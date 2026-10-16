---
id: TASK-098
title: 'Logging: use the base64 crate (no_std) instead of the hand-written codec'
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
- [ ] #1 The codec and its equivalence tests are deleted; grammar and golden tests remain
- [ ] #2 AUDIO/EXCDATA bytes are identical (rig capture re-run)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Adding base64 (default-features = false) as a firmware dependency is fine.
<!-- SECTION:NOTES:END -->
