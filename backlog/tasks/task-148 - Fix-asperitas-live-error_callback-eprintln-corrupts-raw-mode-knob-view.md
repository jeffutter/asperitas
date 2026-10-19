---
id: TASK-148
title: 'Fix: asperitas-live error_callback eprintln corrupts raw-mode knob view'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:35'
updated_date: '2026-10-19 23:52'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 283700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
backend.rs error_callback uses eprintln! from a cpal thread while the terminal is in raw mode. A bare newline staircases the output and the next draw() MoveUp lands on the wrong row. Record the error into the counters/a shared last-error slot and render it in the view (or print it after leaving raw mode) instead.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-148. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
error_callback now stores the latest error in Counters.last_error (Mutex) instead of eprintln!. wait() gets a status reader; the knob view has a constant-height trailing status line (flattened, capped at 78 chars) and redraws when the error changes. backend::run prints the last error once after streams drop and raw mode is restored. Unit tests cover constant line count and truncation.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Stream errors no longer print from the cpal thread in raw mode; they show in the knob view status line and print once after the terminal is restored.
<!-- SECTION:FINAL_SUMMARY:END -->
