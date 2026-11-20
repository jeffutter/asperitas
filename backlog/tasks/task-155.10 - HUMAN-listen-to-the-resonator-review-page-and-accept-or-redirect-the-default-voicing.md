---
id: TASK-155.10
title: >-
  HUMAN: listen to the resonator review page and accept or redirect the default
  voicing
status: To Do
assignee:
  - '@human'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - review
dependencies:
  - TASK-155.09
documentation:
  - docs/reference/review.md
parent_task_id: TASK-155
priority: medium
type: task
ordinal: 305700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Whether the resonator sounds right for mandolin, octave mandolin, upright bass, bass guitar and jazz guitar cannot be shown by tests or plots; it needs the owner's ears. The review page (TASK-155.09) is built so this listening can be done asynchronously, away from the board. This ticket is that listening and its outcome: accept the default voicing and parameter ranges as the baseline, or say what to change.

Changes the owner asks for are filed as new subtasks of TASK-155 (a voicing or range change touches the goldens and the review page, so it is its own piece of work), not done inside this ticket. An agent must not pick this up unattended and must never mark it Done without the owner having walked through each criterion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: the owner has listened to the review page for every instrument family and recorded in the ticket notes which cases were fine and which were not
- [ ] #2 HUMAN: the default voicing and the damping and structure ranges are accepted as the baseline, or each requested change is filed as its own subtask of TASK-155 and linked here
- [ ] #3 HUMAN: any request that the default voicing be re-derived is recorded with the instrument and the symptom (for example too bright, too long, wrong pitch relationship)
- [ ] #4 Close note records that it was done live with the owner, so the history shows a person signed off
<!-- AC:END -->
