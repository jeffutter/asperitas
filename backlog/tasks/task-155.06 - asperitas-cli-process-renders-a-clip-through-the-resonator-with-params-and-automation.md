---
id: TASK-155.06
title: >-
  asperitas-cli process renders a clip through the resonator, with --params and
  --automation
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - cli
dependencies:
  - TASK-155.05
documentation:
  - docs/reference/automation.md
  - audio/README.md
  - crates/asperitas-cli/src/process.rs
parent_task_id: TASK-155
priority: high
type: feature
ordinal: 301700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The device-free-first rule says new DSP must be runnable from `asperitas-cli` on a WAV before it touches the board, and an agent can only check or demonstrate the resonator if it can render a clip through it. `asperitas-cli process` currently selects between gain, filter and the Asperitas patch by processor name. This adds the resonator as a selectable processor using the resonator controls (TASK-155.05), with the same `--params key=value` syntax and the same `--automation` file as the existing patch, so one render path serves analysis, goldens, the review page and later equivalence checks.

Out of scope: the golden files themselves (separate ticket), the review page, live/rig/firmware wiring.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 asperitas-cli process accepts the resonator as a processor and renders an input WAV to an output WAV; with the existing processors the output is unchanged and every existing golden still passes
- [ ] #2 --params accepts the resonator's real-unit keys through the shared translation, refusing an unknown key or out-of-range value with a message naming it
- [ ] #3 --automation drives the resonator's controls with changes latched on the 32-frame block grid; a test shows the output equals the per-block controls applied directly to the Processor, with a negative control for applying a change at the exact frame
- [ ] #4 Rendering the same input and params twice gives bit-identical output (determinism test)
- [ ] #5 The CLI usage and the resonator keys are documented in the resonator docs/reference page and docs/reference/automation.md
- [ ] #6 bash scripts/gates.sh push passes
<!-- AC:END -->
