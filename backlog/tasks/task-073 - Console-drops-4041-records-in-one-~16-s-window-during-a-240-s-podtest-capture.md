---
id: TASK-073
title: Console drops 4041 records in one ~16 s window during a 240 s podtest capture
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 12:05'
labels:
  - bug
dependencies: []
priority: medium
ordinal: 159800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found in TASK-033.01: captures/task-033-01-podtest-240s.txt, bad_frames=0 but seq_gap 0000099f->00001017 (device t=24501 ms to 41017 ms), STATUS at 41017 shows dropped_full=4041 bytes_dropped=253264 pipe_free=2048, discarded_bytes=42. Device-side loss counters caught it (self-verifying transport works), but the cause is unknown: host not draining (macOS port stall) or device-side USB stall. Investigate by repeating the capture and checking whether the gap recurs.
<!-- SECTION:DESCRIPTION:END -->
