---
id: TASK-155
title: >-
  M5: the resonator - a sympathetic resonator effect beside the existing patch,
  from DSP core to the Pod
status: To Do
assignee:
  - '@human'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
priority: high
type: feature
ordinal: 295700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The shipped effect is a one-pole low-pass then gain, which only proves the chain works end to end. The project exists for a sympathetic resonator: a bank of tuned resonators excited by the input signal (Rings-like), for acoustic and clean electric instruments. This umbrella tracks milestone M5 of backlog/docs/doc-001 - Asperitas-Project-Plan.md section 3.

The resonator is added beside `Asperitas`, never replacing it, and follows the device-free-first rule (doc-001 section 4): every piece is exercisable on the host (CLI render, measurement, goldens, review page) before it touches the board. The parameter model stays as in M4: every surface translates its input into a controls type through `ParamSpec` tables, and controls latch on the 32-frame block grid.

The owner decision on tuning model and Pod mapping is its own subtask; the pure DSP pieces do not wait on it. M6 (envelope follower, random-walk modulation) builds on the resonator's parameters and is out of scope here.

This umbrella is @human because its last subtasks need ears and the board. It has no work of its own: an agent must not pick it up. Close it with the owner once every subtask is Done, and update doc-001 section 3 (M5 outcome, anything not done) at that point.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: every subtask of this ticket is Done
<!-- AC:END -->
