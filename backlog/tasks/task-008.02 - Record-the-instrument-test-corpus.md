---
id: TASK-008.02
title: Record the instrument test corpus
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
labels: []
dependencies:
  - TASK-008.01
parent_task_id: TASK-008
type: task
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires instruments and a human playing them. An agent cannot produce this.

Record short reference passages on the instruments this effect is actually for: mandolin, octave mandolin, upright bass, bass guitar, jazz guitar. Include material that exercises what a sympathetic resonator responds to — single plucked notes with long decay, fast passages, chords, and dynamic contrast between soft and hard attack.

Corpus policy, so the repo stays manageable: **2–5 s per clip, mono, 48 kHz**, committed to git.

These are the files golden tests and ear checks run against for the rest of the project, so it is worth recording them cleanly and at consistent level.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: short mono 48 kHz clips recorded across the target instruments
- [ ] #2 HUMAN: material includes long-decay plucks, fast passages, chords, and soft/hard dynamic contrast
- [ ] #3 Clips committed under audio/ and golden tests extended to cover them
<!-- AC:END -->
