---
id: TASK-038.07.01
title: Add a ring-fill capture mode to rig that records until no Free block remains
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-08 13:56'
updated_date: '2026-10-08 14:31'
labels:
  - planned
  - ready-for-agent
dependencies: []
parent_task_id: TASK-038.07
priority: medium
ordinal: 127800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rig's capture window is a build-time number of seconds, const-gated to fit inside the ring (rig.rs, `WINDOW_BLOCKS < capture::RING_BLOCKS`). TASK-038.07 needs a run that deliberately fills the ring, so the device's claim about maximum capturable duration (`CAPMAX seconds_max`) can be checked against the wall clock.

Add a build-time way to select 'capture until the ring is full'. Suggested: `ASP_RIG_CAPTURE_SECONDS=ring`, parsed by the existing const parser, so there is still one knob and junk still fails the build. In that mode the window gate does not apply and the capture ends when the producer finds no Free block. The producer already does the right thing there: it disarms, counts one overrun and never overwrites a block (`Producer::claim`). So the work is in the timeline (`run_capture`) and the judging (`judge_capture`):
- End the window on the producer's disarm instead of a deadline.
- Expect exactly `capture::RING_BLOCKS` delivered.
- Report the overrun that ended it as the expected terminator, not as a FAIL.
- Log the device's own elapsed capture time in ms (first armed callback to disarm), so the bench can set it beside `CAPMAX seconds_max` and the wall clock.

Bench findings from 2026-10-07 that apply: a full-ring dump is about 59 MB on the wire, about 81 s at the measured 731.7 kB/s (TASK-038.05 notes). The default 300 s build must stay byte-for-byte what it is, and its gates unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `ASP_RIG_CAPTURE_SECONDS=ring` (or an equivalent single build-time knob) builds a rig that captures until no Free block remains, and any value that is neither a whole number of seconds nor that word still fails the build
- [ ] #2 In ring mode, the capture ends on the producer's disarm, the judge expects delivered == capture::RING_BLOCKS, reports the terminating overrun as expected rather than as a failure, and logs the measured capture duration in ms from the first armed callback to the disarm
- [ ] #3 The default (300 s) build keeps its gates and its timeline unchanged, and a host-checkable test or const assert covers the parser's new arm
- [ ] #4 scripts/gates.sh gains a push-tier build of the ring-mode variant, and `scripts/gates.sh push` passes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Also discharges the 'or until the ring reports full' clause of TASK-038.03 #10 and TASK-038.03.02 #9, which were closed on 2026-10-08 with that clause moved here.
<!-- SECTION:NOTES:END -->
