---
id: TASK-073
title: Console drops 4041 records in one ~16 s window during a 240 s podtest capture
status: Done
assignee:
  - '@human'
created_date: '2026-10-15 12:05'
updated_date: '2026-10-15 14:11'
labels:
  - bug
  - planned
dependencies:
  - TASK-073.01
priority: medium
ordinal: 159800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found in TASK-033.01: captures/task-033-01-podtest-240s.txt, bad_frames=0 but seq_gap 0000099f->00001017 (device t=24501 ms to 41017 ms), STATUS at 41017 shows dropped_full=4041 bytes_dropped=253264 pipe_free=2048, discarded_bytes=42. Device-side loss counters caught it (self-verifying transport works), but the cause is unknown: host not draining (macOS port stall) or device-side USB stall. Investigate by repeating the capture and checking whether the gap recurs.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 7a2ad00. Umbrella: the cause (host drain vs device USB stall) can only be learned from repeat hardware captures, done in TASK-073.01 (@human). Once its result is in, a follow-up ticket for the fix (or closing as not reproducible) is created from its conclusion. Acceptance: TASK-073.01 done and conclusion recorded.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Conclusion (agreed with the owner in a live session, 2026-10-15): not a mid-run stall, neither host drain nor device USB stall. Three 240 s captures (captures/task-073-run-{1,2,3}.txt, commit 70921f2, details in TASK-073.01 notes) show dropped_full flat for the entire capture in every run. Drops and sequence gaps occur only before or at host attach: ~30 records after a fresh boot, and a gap proportional to how long the device ran unread (8.6 s after ~57 min, which also explains TASK-033.01's 16 s window). No fix needed on the device. Caveat: not tested with the Mac display awake (owner waived; all runs had it off over ssh).

Minor side effect seen: the rig verdict fails on runs started against a freshly booted device because of discarded_bytes (2-11 bytes, partial frame at stream start). If that matters for future tickets, restart the device before capturing and allow for it, or have the rig tolerate leading partial-frame bytes.
<!-- SECTION:NOTES:END -->
