---
id: TASK-005.02
title: Verify audio in and out on real hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
labels: []
dependencies:
  - TASK-005.01
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/daisy-pod.md
parent_task_id: TASK-005
type: feature
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board, the Pod, and ears. Reproduce the checks PR #80's author ran on this same Seed3-in-Pod configuration.

The Pod's audio I/O is **line level, not hi-Z instrument level** — feed it from a DI, preamp, or an interface's line out. Do not plug a mandolin pickup straight in and conclude the codec is broken. See docs/reference/daisy-pod.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: audio into the Pod is audible at its output
- [ ] #2 HUMAN: left and right channels verified not swapped
- [ ] #3 HUMAN: line-out to line-in loopback passes audio repeatably across resets
- [ ] #4 HUMAN: board boots and resets reliably; USB connects and reconnects
- [ ] #5 Any deviation from the SAI config in docs/reference/daisy-seed3.md is corrected there
<!-- AC:END -->
