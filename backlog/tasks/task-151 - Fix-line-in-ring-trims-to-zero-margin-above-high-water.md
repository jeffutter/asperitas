---
id: TASK-151
title: 'Fix: line-in ring trims to zero margin above high water'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:58'
updated_date: '2026-10-20 00:05'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 286700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-live/src/ring.rs RingSync::decide trims to exactly one host buffer (occupied - wanted) so the ring is empty after the callback pops it. Any later or smaller input block then underruns, re-primes with a full buffer of silence, and with clock drift this repeats as a periodic glitch. Trim to a target with margin (e.g. wanted + a fraction of high water) and update the trims_back_to_one_buffer test. Also consider not counting the initial priming Trim in the drift trims counter.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-151. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added RingSync::trim_target (wanted + half headroom to high water); drift and priming trims leave that margin. Priming surplus is a new RingAction::Prime, not counted in the trims counter. Tests and live doc updated.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Line-in ring trims to a margin above one host buffer instead of zero; priming trims no longer count as drift.
<!-- SECTION:FINAL_SUMMARY:END -->
