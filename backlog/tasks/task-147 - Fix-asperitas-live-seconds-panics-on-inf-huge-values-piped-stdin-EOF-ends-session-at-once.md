---
id: TASK-147
title: >-
  Fix: asperitas-live --seconds panics on inf/huge values; piped stdin EOF ends
  session at once
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:35'
updated_date: '2026-10-19 23:49'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 282700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
terminal::wait does Duration::from_secs_f64(s.max(0.0)) and Instant::now()+..., which panic for inf, NaN or 1e300. Validate --seconds in the arg parser (finite, positive, bounded) and return a clean error. Also the non-terminal fallback ignores read_line EOF, so with stdin closed the session ends immediately; on EOF with no --seconds, wait indefinitely (or report it).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-147. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
parse_seconds value_parser (finite, >0, <=86400) in main.rs; terminal::wait uses try_from_secs_f64/checked_add; non-terminal EOF with no --seconds parks forever. Unit tests added; --seconds inf gives a clean error.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Bad --seconds values are rejected at parse time; wait is overflow-safe; closed stdin no longer ends the session.
<!-- SECTION:FINAL_SUMMARY:END -->
