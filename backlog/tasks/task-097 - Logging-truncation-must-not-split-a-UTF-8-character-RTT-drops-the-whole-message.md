---
id: TASK-097
title: >-
  Logging: truncation must not split a UTF-8 character (RTT drops the whole
  message)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 23:12'
labels:
  - review-2026-10
  - logging
  - planned
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
- [x] #1 A test shows a message truncated mid-character is emitted as its valid prefix, not dropped
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by a290b8f. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added utf8_prefix in asperitas-logging lib.rs; defmt emit_frame now uses it, removing the duplicated five-arm marker match. Body cut mid-character emits its valid prefix; marker only when nothing valid precedes. Tests in utf8_prefix_tests incl. real BodyWriter truncation. Firmware checked with seed3 log-defmt.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
defmt sink emits the valid UTF-8 prefix of a mid-character-truncated body instead of dropping it.
<!-- SECTION:FINAL_SUMMARY:END -->
