---
id: TASK-146
title: 'Fix: asperitas-live doc and ParamSpec doc stale after knob view'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:35'
updated_date: '2026-10-20 00:27'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 281700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
docs/reference/asperitas-live.md still says knobs are fixed for the run and the stream ends on Enter; it does not document q/a, w/s, shift keys or esc, and the --params and --seconds flag rows are wrong. Also the ParamSpec doc in crates/asperitas-dsp/src/param.rs says the range is not enforced for CLI values, but Controls::from_values now rejects out-of-range values.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-146. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
planning marked this Dev Ready, but its dependencies are not all Done, so it was forced to Blocked. It should be re-checked once its dependencies are Done.

Doc-only: added Knob view section and fixed --params/--seconds rows in asperitas-live.md; corrected ParamSpec doc.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Updated asperitas-live.md and ParamSpec doc to match the knob view and Controls::from_values validation.
<!-- SECTION:FINAL_SUMMARY:END -->
