---
id: TASK-155.07
title: >-
  Analysis output for the resonator: impulse and frequency response from the CLI
  or asperitas-measure
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - tooling
dependencies:
  - TASK-155.06
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/asperitas-rig.md
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 302700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The testing strategy says seeing a DSP bug is easier than hearing it, and the plan asks for impulse and frequency response output so an agent or the owner can see what the bank does at a given setting without listening. Without it, tuning errors, runaway ringing and the effect of damping and structure are invisible to anything but ears.

Produce, for a chosen set of resonator parameters (the same `key=value` syntax as `--params`), an impulse response and a magnitude frequency response in a form that can be inspected and diffed: numeric output (CSV or the format `asperitas-measure` already uses) and a plot that can be embedded in the review page (TASK-155.09). It should show the tuned peaks, their decay and the left/right difference from stereo spread. Use whichever of `asperitas-cli` or `asperitas-measure` fits the existing analysis code; record the choice and why in the ticket notes.

Out of scope: rig captures of the device, goldens, the review page itself.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A command produces the resonator's impulse response and magnitude frequency response for given real-unit parameters, as numeric output and as a plot file
- [ ] #2 A test on the default voicing shows the frequency-response peaks sit at the tuned pitches within a stated tolerance, and a changed damping or structure parameter moves them as documented
- [ ] #3 The numeric output is deterministic: the same parameters give byte-identical output
- [ ] #4 A bad or out-of-range parameter is refused with a message naming it
- [ ] #5 The command, its output format and an example are documented in docs/reference, with the decision on CLI versus asperitas-measure recorded in the ticket notes
- [ ] #6 bash scripts/gates.sh push passes
<!-- AC:END -->
