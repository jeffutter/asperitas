---
id: TASK-152.01
title: >-
  asperitas-cli process renders a clip with scripted knob changes from an
  automation file
status: To Do
assignee:
  - '@agent'
created_date: '2026-11-13T22:00:00Z'
labels:
  - automation
  - cli
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/asperitas-live.md
  - docs/reference/asperitas-rig.md
  - audio/README.md
parent_task_id: TASK-152
priority: high
type: feature
ordinal: 288700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An agent cannot produce a clip in which a knob moves, so it cannot check or demonstrate anything that depends on how the effect responds to knob motion (smoothing, zipper noise, later a resonator's damping sweep) without the board or a person. `asperitas-cli process` only accepts fixed `--params`; the rig `session` command has hold, sweep and soak schedules that exist only on the device path.

Add an automation input to the offline render: a file describing, per knob, a list of time/value breakpoints with the values in the same real units and `key=value` names that `--params` uses, resolved through the `ParamSpec` tables to `Controls`, with changes latched on the 32-frame block grid counted from the start of the clip, exactly as `asperitas-live` and firmware `main` do. The file format must be small, human- and LLM-writable, and documented with an example; it is the one definition of knob motion that later tickets (equivalence test, review package, rig session) reuse, so it must be able to express hold, linear ramp between breakpoints and a step. Before building a parser or format, name the existing standard or library in the dependency tree that already does it and record that in the ticket notes, even if the decision is to build it.

Out of scope: any resonator or new processor; reading automation anywhere other than the offline render (separate tickets).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `asperitas-cli process` accepts an automation file alongside the input and output WAVs and the output has the knob changes applied; with no automation file its output is unchanged and every existing golden still passes
- [ ] #2 A knob value in the automation file is given in real units and mapped through the ParamSpec tables to Controls; a value outside a ParamSpec range, an unknown knob name, unsorted times and a malformed file are each refused with a message naming the problem and the line or key, never clamped or ignored
- [ ] #3 Hold, linear ramp between breakpoints and a step are each expressible, with a test per form that checks the applied Controls at frames before, on and after each breakpoint
- [ ] #4 Changes land on the next 32-frame block boundary after the requested time; a test places a breakpoint at a frame that is not a multiple of 32 and asserts the output equals the Controls applied at the following boundary, and a negative control shows applying it at the exact frame gives a different output
- [ ] #5 The automation format, an example file and the 32-frame latch rule are documented in a docs/reference page and linked from CLAUDE.md's where-things-live list
- [ ] #6 Ticket notes name the standard or library considered for the file format and why it was or was not used
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
