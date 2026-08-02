---
id: TASK-008.02
title: Record the instrument test corpus
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-02 01:53'
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

Record short reference passages for mandolin and octave mandolin. These are the files golden tests and ear checks run against for the rest of the project, so it is worth recording them cleanly and at consistent level.

**Format:** mono, 48 kHz WAV, ~3 seconds each, committed to `audio/`. Keep volume consistent across all clips.

### Mandolin (4 clips)

| File | What to play |
|------|-------------|
| `mandolin_single_note_soft.wav` | Single open A string, gentle pick, let ring |
| `mandolin_single_note_hard.wav` | Same note, aggressive pick attack |
| `mandolin_fast_run.wav` | Fast scale run (A minor: A-C-D-E over 2 beats) |
| `mandolin_chord.wav` | Strummed D major chord, let decay |

### Octave Mandolin (4 clips)

| File | What to play |
|------|-------------|
| `octave_mandolin_single_note_soft.wav` | Single open A string, gentle pick, let ring |
| `octave_mandolin_single_note_hard.wav` | Same note, aggressive pick attack |
| `octave_mandolin_fast_run.wav` | Fast scale run (A minor, same as mandolin) |
| `octave_mandolin_chord.wav` | Strummed D major chord, let decay |
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: short mono 48 kHz clips recorded across the target instruments
- [ ] #2 HUMAN: material includes long-decay plucks, fast passages, chords, and soft/hard dynamic contrast
- [ ] #3 Clips committed under audio/ and golden tests extended to cover them
<!-- AC:END -->
