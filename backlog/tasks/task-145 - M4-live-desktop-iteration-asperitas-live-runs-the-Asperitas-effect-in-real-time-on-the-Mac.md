---
id: TASK-145
title: >-
  M4: live desktop iteration - asperitas-live runs the Asperitas effect in real
  time on the Mac
status: Done
assignee:
  - '@human'
created_date: 2026-10-19 22:07
updated_date: '2026-11-13T21:10:00Z'
labels:
  - m4
  - live
dependencies:
  - TASK-145.01
  - TASK-145.02
  - TASK-145.03
  - TASK-145.04
  - TASK-145.05
  - TASK-145.06
  - TASK-145.07
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
priority: high
type: feature
ordinal: 280000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella for milestone M4 (doc-001 section 3). Today the only device-free path is offline: `asperitas-cli process` runs a WAV through the effect with `--params`, and goldens pin the output. Every sound-design change that needs listening costs a flash cycle to the Pod. M4 adds a desktop host, `asperitas-live`, that runs the same `Asperitas` effect in real time through `cpal`, with live knob control from a thin `crossterm` terminal view (owner decision 2026-10-19: crossterm only, no ratatui).

Exit criterion (doc-001): the live host and the device, given the same `Controls` and input, produce the same output within float tolerance.

This umbrella is @human because its last subtasks (bench comparison and listening check) need the board and ears. It has no work of its own: an agent must not pick it up. Close it with the owner once every subtask is Done.

Out of scope: any M5 (resonator) work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: every subtask of this ticket is Done
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-13: Closed with the owner once every subtask was Done. Exit criterion evidence: the live engine matches asperitas-cli and ReplayFx bit for bit (TASK-145.05), the board matches the same reference on real audio at cutoff 2 kHz and gain -6 dB (TASK-145.06, captures/TASK-145.06/), and the owner listened to a --record capture and found loops clean and knobs smooth (TASK-145.07). Deferred by the owner and tracked in TASK-152: an instrument on a real line-in with latency judged, and a by-ear comparison against the Pod running main.
<!-- SECTION:NOTES:END -->
