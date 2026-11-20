---
id: TASK-155.09
title: >-
  Review page for the resonator: rendered clips, knob sweeps and response plots
  for asynchronous listening
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - review
dependencies:
  - TASK-155.06
  - TASK-155.07
documentation:
  - docs/reference/review.md
  - docs/reference/automation.md
  - audio/README.md
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 304700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
How the resonator sounds can only be judged by ear, and the owner wants to do that asynchronously rather than at a live bench session. `asperitas-cli review` (docs/reference/review.md) renders a manifest of cases into one self-contained HTML page. This ticket writes the resonator's manifest and any glue it needs, so an agent can hand the owner one page: each corpus instrument clip dry and through the default voicing, damping and structure sweeps driven by an automation file, and the impulse and frequency response plots (TASK-155.07) beside them.

The manifest and its automation files are committed so the page can be regenerated after a voicing change. The page is a deliverable of this ticket and its generation command is documented; the listening itself is TASK-155.10.

Out of scope: judging the sound, changing the default voicing (that follows the owner's listen).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A committed review manifest renders, with one command, a self-contained page with each corpus instrument clip dry and through the resonator default voicing
- [ ] #2 The page includes at least one damping sweep and one structure sweep per instrument family, each driven by a committed automation file and passing the review command's parameter-step check
- [ ] #3 The page embeds the impulse and frequency response plots from the analysis command (TASK-155.07)
- [ ] #4 The page opens offline with no external requests, and its generation is deterministic
- [ ] #5 The command to regenerate the page and how to open it are documented in docs/reference/review.md or the resonator docs page
- [ ] #6 bash scripts/gates.sh push passes
<!-- AC:END -->
