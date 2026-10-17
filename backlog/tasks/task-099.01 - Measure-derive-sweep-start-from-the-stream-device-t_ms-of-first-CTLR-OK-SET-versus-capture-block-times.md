---
id: TASK-099.01
title: >-
  Measure: derive sweep start from the stream (device t_ms of first CTLR OK SET
  versus capture block times)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-17 02:58'
labels:
  - planned
dependencies: []
parent_task_id: TASK-099
priority: medium
ordinal: 245800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner decision on TASK-099: motion timing comes from the stream itself if that is enough. Establish whether the framed stream carries enough: the t_ms of the first CTLR OK SET record and a device time for the capture's first PCM block. If yes, add a loader in asperitas-host/asperitas-measure that returns first-SET time as an offset in seconds from capture start, replacing the hand-set --sweep-start-s and letting thresholds.rs drop the SWEEP_BUCKET_TRIM allowance for the offset estimate. If the stream cannot supply it, record that in the ticket notes and the session sidecar (next ticket) must carry it. AC: offset computed from a stream with a known fixture; tests; fallback decision recorded.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by eb40027. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decision: the stream carries t_ms on every frame, so the first 'CTLR OK SET' reply gives a device time for sweep start (asperitas_host::timing::first_set_t_ms). It does NOT carry a device time for capture start: AUDIO/AUDEND frames are emitted after the capture finishes (t_ms is dump time) and no record stamps the first captured block. timing::sweep_offset_s therefore returns TimingError::NoCaptureClock. Fallback: the session sidecar (TASK-099.02) must carry a host-side pairing of capture start and a device t_ms (taken when asperitas-rig issues the CTL) plus capture-start host time. Alternative for the owner, not done: a firmware CAPSTART record with t_ms of the first captured block (wire + firmware change, needs hardware verification). SWEEP_BUCKET_TRIM and --sweep-start-s untouched (TASK-099.03).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-host timing module: first_set_t_ms (first CTLR OK SET device t_ms) and sweep_offset_s (typed NoSweep/NoCaptureClock). Tests cover first-of-many SET, no SET, and AUDIO frames not providing a capture clock. Decision recorded: sidecar must carry capture start.
<!-- SECTION:FINAL_SUMMARY:END -->
