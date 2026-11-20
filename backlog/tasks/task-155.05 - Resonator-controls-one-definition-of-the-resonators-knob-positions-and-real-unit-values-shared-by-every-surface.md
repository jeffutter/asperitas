---
id: TASK-155.05
title: >-
  Resonator controls: one definition of the resonator's knob positions and
  real-unit values, shared by every surface
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - dsp
dependencies:
  - TASK-155.04
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/reference/automation.md
  - crates/asperitas-dsp/src/asperitas.rs
  - docs/reference/asperitas-live.md
parent_task_id: TASK-155
priority: high
type: feature
ordinal: 300700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
M4 established that every surface (Pod knobs, `asperitas-cli --params`, `CTL SET`, the live host) translates its input into one `Controls` definition through the `ParamSpec` tables, so "same as the device" means exactly one definition. `Controls` and `ASPERITAS_PARAMS` today are specific to the two-knob low-pass and gain patch (`KNOB_COUNT` is 2). The resonator needs its own equivalent that follows the Pod mapping the owner chose in TASK-155.01 (which parameters ride on the knobs and which on the encoder and buttons), built so that adding it does not change what `Asperitas` or any existing host does.

Provide, in `asperitas-dsp`, the resonator controls type: normalised positions (clamped, NaN falls back to the parameter default), construction from real-unit `key=value` pairs that refuses unknown keys and out-of-range values with a message naming the problem (never clamping), the real-unit values for display, and the translation into the resonator's `Params`. Controls latch on the 32-frame block grid as in M4; this ticket does not change that rule. Also make the existing automation format (docs/reference/automation.md, `Automation::controls_at`) able to drive the resonator's controls, so scripted knob motion works for it, or record in the ticket notes why it needs a separate path.

Out of scope: wiring into any host binary (CLI, live, rig, firmware are separate tickets).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A resonator controls type in asperitas-dsp maps normalised positions to real units through the resonator ParamSpec table and to the resonator's Params, matching the mapping in the TASK-155.01 decision record
- [ ] #2 Out-of-range knob positions clamp and NaN falls back to the default; from_values refuses an unknown key, a non-finite value and an out-of-range value, each with a message naming the parameter, and a test covers each refusal
- [ ] #3 A round-trip test shows real value -> knob position -> real value agrees within float tolerance for every parameter and taper
- [ ] #4 The automation format can express hold, ramp and step on the resonator's controls with the same 32-frame latch rule, with a test at frames before, on and after a breakpoint; or the ticket notes record why a separate path was chosen
- [ ] #5 Asperitas, Controls, ASPERITAS_PARAMS and every existing golden, equivalence test and automation test are unchanged and passing
- [ ] #6 docs/reference documents the resonator parameter names, ranges, units and the knob/encoder/button mapping, and automation.md names the resonator's keys
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
