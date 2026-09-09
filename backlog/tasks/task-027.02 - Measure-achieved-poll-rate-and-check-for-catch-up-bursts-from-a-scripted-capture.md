---
id: TASK-027.02
title: >-
  Measure achieved poll rate and check for catch-up bursts from a scripted
  capture
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:29'
updated_date: '2026-09-09 01:31'
labels: []
dependencies:
  - TASK-027.01
  - TASK-030
  - TASK-031
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-027
priority: high
type: task
ordinal: 112
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-027's four criteria are measurements: take a capture, compute the median interval between knob records, check for catch-up bursts, write down the number. They carry a `HUMAN:` prefix because nobody could run the capture unattended, not because they need judgement.

With a runner that produces integrity-checked captures, the only human part is flashing and leaving the board cabled — which TASK-027.01 isolates. This ticket is what remains: the analysis, done by a program, reporting a number instead of an impression. That distinction matters here specifically — TASK-025 was once marked Done while its own hardware criterion sat unchecked, and a recorded measurement is harder to quietly skip than a checkbox.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A capture of at least 240 s is taken through the rig runner and its integrity summary shows no failed frames, so the interval measurement below rests on trustworthy records.
- [ ] #2 The median inter-record interval for knob records is computed from timestamps and reported, with spread; roughly 10 ms is expected for KNOB_LOG_THROTTLE=10 at a true 1 kHz poll rate, not the ~16 ms of the pre-TASK-025 625 Hz build.
- [ ] #3 No back-to-back burst of knob records with near-zero interval appears — the failure mode TASK-026 bounds. If one does appear, a bug ticket is filed carrying the timestamps from the artifact, and this criterion is left unchecked.
- [ ] #4 Median and spread are recorded as numbers in TASK-027's implementation notes, and TASK-025 AC #3 is checked via task_edit only once the measured interval supports it.
- [ ] #5 The capture artifact is committed or attached so the number can be re-checked rather than trusted.
<!-- AC:END -->
