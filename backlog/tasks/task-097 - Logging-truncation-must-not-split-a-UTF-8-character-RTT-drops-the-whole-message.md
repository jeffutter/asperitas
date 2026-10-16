---
id: TASK-097
title: >-
  Logging: truncation must not split a UTF-8 character (RTT drops the whole
  message)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
dependencies: []
priority: low
type: bug
ordinal: 199800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TruncWriter::write_str cuts at a byte count (lib.rs:183-189); defmt_log::emit_frame then replaces the entire body with '<invalid utf8>' (defmt_log.rs:179-188). A long non-ASCII log line, or a panic message cut at PANIC_MSG_BUF = 128 that contains a path, is lost entirely over RTT.

Change: truncate on a char boundary or emit the valid_up_to() prefix; this also removes the duplicated five-arm match.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A test shows a message truncated mid-character is emitted as its valid prefix, not dropped
<!-- AC:END -->
