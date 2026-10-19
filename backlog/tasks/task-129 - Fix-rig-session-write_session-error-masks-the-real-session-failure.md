---
id: TASK-129
title: 'Fix: rig session write_session error masks the real session failure'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 03:09'
updated_date: '2026-10-19 13:51'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 252800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-rig/src/main.rs runs write_session(...)? before reporting run.error, so an I/O failure on the output dir replaces the real Timeout/Closed/Rejected error; an empty raw stream writes no file and prints nothing. Report the session error first (or both) and say when no artifact was written.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
write_session now returns io::Result<bool> (false = empty raw stream, nothing written). New session_error() gives the session's own failure precedence over an artifact write failure. main prints the summary, reports the write failure on stderr and 'no artifact written' for the empty case. Tests added for both.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Session errors are no longer masked by write_session failures; empty raw streams report that no artifact was written.
<!-- SECTION:FINAL_SUMMARY:END -->
