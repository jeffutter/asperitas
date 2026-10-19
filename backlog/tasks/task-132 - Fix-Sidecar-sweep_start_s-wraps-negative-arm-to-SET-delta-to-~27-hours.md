---
id: TASK-132
title: 'Fix: Sidecar::sweep_start_s wraps negative arm-to-SET delta to ~27 hours'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 20:13'
updated_date: '2026-10-19 14:06'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 255800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-host/src/session.rs sweep_start_s uses rem_euclid over a 100,000,000 ms modulus on first_set_t_ms - arm_t_ms. If the first SET reply precedes the arming CAPSTAT record (arm_t_ms is the first periodic CAPSTAT with delivered>0, so it can lag real arming), the result is ~99,999 s and the analysis judges a window outside the capture. Only hold masks this. Treat a delta beyond a plausible bound as unreadable and fall back to start_after_arm_s (or clamp), with a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 319e5e8. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
motion_start_s now rejects a device delta above start_after_arm_s + 30 s (MAX_SET_LATENCY_S) and returns start_after_arm_s. Tests cover reply-before-arm and the bound. Only asperitas-measure calls it.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Negative arm-to-SET delta no longer wraps to ~27 h; falls back to the planned delay. Commit 319e5e8.
<!-- SECTION:FINAL_SUMMARY:END -->
