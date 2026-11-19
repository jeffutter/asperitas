---
id: TASK-152
title: >-
  M4.5: scripted knob automation and device-free audio review, so sound work
  needs the board and a person as little as possible
status: Done
assignee:
  - '@human'
created_date: '2026-11-13T22:00:00Z'
updated_date: '2026-11-19T03:40:00Z'
labels:
  - automation
  - review
  - tooling
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/asperitas-live.md
  - docs/reference/asperitas-rig.md
priority: high
type: feature
ordinal: 287700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The next milestones (M5 resonator, M6 modulation) are audio work that is judged by ear, and today an agent cannot render a clip in which a knob moves: `asperitas-cli process` takes fixed parameter values only, and knob motion exists only in the rig's `session` command (hold, sweep, soak on the board) and in the live keyboard view. Every such check therefore needs the device or a person at the keyboard.

This umbrella gives agents a device-free loop and gives the owner an asynchronous one: an automation file describes knob changes over time; one offline render turns a clip plus that file into a WAV; a review command renders a set of clips and plots into one page for the owner to listen to when convenient; and the rig session reads the same file so the device can be compared against the offline render at moving settings. It follows the device-free-first rule now in doc-001 section 4.

Decisions already made with the owner (2026-11-13): scripted automation is wanted; a web UI that drives the device is not (the TUI and host control already cover it); a wasm/browser build of the effect is not planned and may be revisited. The parameter model stays as in M4: every surface translates its input into `Controls` through the `ParamSpec` tables, and controls latch on the 32-frame block grid.

This umbrella is @human because its last subtask is a bench run. It has no work of its own: an agent must not pick it up. Close it with the owner once every subtask is Done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: every subtask of this ticket is Done
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-18: Closed live with the owner. All five subtasks are Done (152.01 to 152.04 by the loop, 152.05 on the board with the owner); the offline render, the live-engine equivalence test, the review page and the rig automation session all exist, and the board agrees with the offline render (captures/TASK-152.05/).
<!-- SECTION:NOTES:END -->
