---
id: TASK-154
title: 'Fix: review step check misses bare default-step lines and exempts whole script'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-19T02:56:00Z'
updated_date: '2026-11-19T03:06:00Z'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 294700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-cli/src/review.rs has_step_event counts only lines whose second token is the literal 'step'. A line with no keyword is a step by the parser default, so '0 cutoff_hz=1000' then '2 gain_db=-12' is not recognised and the case fails the 'parameter steps' check. Also, one step line disables the parameter-step check for the whole case, so ramp zipper artefacts elsewhere pass silently. Derive step events from the parsed Automation (not text tokens) and exempt only the parameter and time of those events.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added Automation::step_frames (non-ramp breakpoints after frame 0, keyword optional). review.rs now parses the automation and excuses only detector events within a window of a step frame; the rest still count against the dry clip. Unit tests on run_checks with a synthetic tone, since the DSP smooths gain and a rendered step does not reliably flag. Docs updated in docs/reference/review.md. Gates commit tier passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
review's parameter-steps check derives step events from the parsed Automation, so bare default-step lines count, and exempts only events near a scripted step instead of the whole case.
<!-- SECTION:FINAL_SUMMARY:END -->
