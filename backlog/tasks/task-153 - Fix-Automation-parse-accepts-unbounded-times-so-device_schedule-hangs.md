---
id: TASK-153
title: 'Fix: Automation::parse accepts unbounded times so device_schedule hangs'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-19T02:56:00Z'
updated_date: '2026-11-19T03:00:00Z'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 293700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Automation::parse in crates/asperitas-dsp/src/automation.rs only checks that the time is finite and non-negative. A line such as '1e12 gain_db=0' makes device_schedule insert a frame every 480 samples into a BTreeSet up to the last frame (about 1e11 entries), so rig session and the sidecar duration computation hang or run out of memory. Bound the script length in parse with a clear error, and make device_schedule sample only the intervals that change (ramps) instead of every 10 ms frame.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 1465fe8. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added MAX_SCRIPT_SECONDS (3600) rejected in parse; device_schedule now samples the 480-frame grid only across ramped spans. Tests added; docs/reference/automation.md updated. gates commit passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
parse bounds script length; device_schedule no longer walks every 10 ms frame to the end of the script.
<!-- SECTION:FINAL_SUMMARY:END -->
