---
id: TASK-006.02
title: Verify serial output and LED polarity on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
labels: []
dependencies:
  - TASK-006.01
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-006
type: feature
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board. Confirm the probe-free debug channel actually works — this is the tooling everything else will be debugged through for the next several weeks, so it needs to be trustworthy before it is relied on.

LED drive polarity cannot be determined without looking at the board; flip the constant from TASK-006.01 if the LEDs read inverted, and record the answer in docs/reference/daisy-pod.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: running firmware prints text visible on the host over USB CDC serial
- [ ] #2 HUMAN: LED boot stages are visually distinguishable
- [ ] #3 HUMAN: a deliberate panic reaches the developer via LED state and via serial
- [ ] #4 LED drive polarity verified and documented in docs/reference/daisy-pod.md
<!-- AC:END -->
