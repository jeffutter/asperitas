---
id: TASK-136
title: >-
  Fix: rig capture header writes first_unparsed_status verbatim and can be
  forged by a newline in device bytes
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 14:02'
updated_date: '2026-10-19 14:27'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 262800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-rig/src/lib.rs render(), first_unparsed_status={body} is written into the line-oriented header before the -- separator. body comes from String::from_utf8_lossy of device bytes, so a STATUS body containing a newline can inject header lines such as verdict= or accounting_ok=. Escape control characters (or truncate to one line) when rendering that field, and add a test with an embedded newline.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
render() now writes first_unparsed_status via escape_default; raw String kept in Summary. Added test with \n, \r\n, \r injection; documented in asperitas-rig.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Escaped first_unparsed_status at render time so device bytes cannot forge header lines; test and doc added.
<!-- SECTION:FINAL_SUMMARY:END -->
