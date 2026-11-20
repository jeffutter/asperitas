---
id: TASK-155.01
title: >-
  HUMAN: decide the resonator's tuning model, voice budget and Pod control
  mapping
status: To Do
assignee:
  - '@human'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - decision
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/daisy-pod.md
parent_task_id: TASK-155
priority: high
type: spike
ordinal: 296700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The plan leaves the knob-to-parameter mapping for the Pod's two knobs, encoder and buttons as an owner decision, and the tuning model (what the resonators are tuned to) decides the resonator's parameter set, its CPU cost and how the instrument is played through it. Building the bank or the Pod mapping before this is settled risks rework, so it is a decision ticket the owner closes in a live session; the pure DSP building blocks (the single resonator voice, the excitation stage) do not depend on it.

Questions to settle with the owner, each with the alternatives and cost recorded: (1) tuning model - a fixed set of pitches per instrument tuning (mandolin, octave mandolin, upright bass, bass guitar, jazz guitar), a root note plus a structure series (harmonic to stretched/inharmonic), or pitch tracked from the input; (2) voice count budget, which sets the CPU target on the Seed3 at 48 kHz with 32-sample blocks; (3) which controls the two knobs, the encoder, its click and the two buttons carry, and what the two RGB LEDs show; (4) the default voicing a first listen should start from. Record the outcome as a decision record in backlog/decisions and link it from doc-001 section 3 M5.

Agents must not pick this up unattended. It needs the owner's choices, not code.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: the tuning model is chosen, with the alternatives considered and the reason recorded in a decision record under backlog/decisions
- [ ] #2 HUMAN: the voice count budget is chosen and recorded, with the CPU target it implies
- [ ] #3 HUMAN: the mapping of the two knobs, encoder, encoder click, two buttons and two RGB LEDs to resonator controls is chosen and recorded
- [ ] #4 HUMAN: the default voicing for the first listening round is recorded
- [ ] #5 The decision record is linked from doc-001 section 3 (M5)
- [ ] #6 Close note records that it was done live with the owner, so the history shows a person signed off
<!-- AC:END -->
