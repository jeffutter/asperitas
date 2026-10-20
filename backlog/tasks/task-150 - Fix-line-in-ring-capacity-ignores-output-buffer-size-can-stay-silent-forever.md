---
id: TASK-150
title: 'Fix: line-in ring capacity ignores output buffer size, can stay silent forever'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:58'
updated_date: '2026-10-20 00:01'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 285700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-live/src/backend.rs sizes the ring from the input stream buffer (or a 16384 floor) but RingSync::decide wants the output callback's frame count. If the output host buffer exceeds the capacity, occupied < wanted always holds, decide returns Wait forever, no underrun is counted and no error shown. Size the capacity from max(input, output) buffer (at least 4x the output buffer) or fail loudly at startup when capacity < 2 * output buffer.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-150. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Ring capacity now from ring_capacity(input, output) in backend.rs: 8x the larger Fixed buffer, floor 4096, Default counts as 16384. RingSync::cannot_prime reports when the ring holds under two output buffers; the output callback then sets last_error and bumps stream_errors once (try_lock, no blocking). Unit tests added; cargo test/clippy/fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Line-in ring is sized from both streams and a Default-sized output too big for the ring now raises a visible stream error instead of staying silent.
<!-- SECTION:FINAL_SUMMARY:END -->
