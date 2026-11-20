---
id: TASK-155.02
title: >-
  A single tuned resonator voice in asperitas-dsp: pitch and decay controls,
  smoothed, no_std
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - dsp
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - crates/asperitas-dsp/src/filter.rs
  - crates/asperitas-dsp/src/processor.rs
parent_task_id: TASK-155
priority: high
type: feature
ordinal: 297700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The resonator bank is made of many identical tuned voices. Getting one voice right (stable at every legal pitch and decay, no clicks when its controls move, bounded output) on its own keeps the bank ticket about composition rather than filter numerics, and lets the building block ship and be tested before the owner decision on tuning model lands.

A voice is a `Processor` in `asperitas-dsp` (no_std, no allocation, no `Result`) that rings at a set pitch when excited and decays over a set time. It follows the crate's conventions: a `ParamSpec` table is the only source of its ranges, tapers and defaults; parameter changes are smoothed inside the processor so no host reimplements it; degenerate inputs (NaN, out of range, pitch at or above Nyquist, zero decay) are sanitised into ordinary values. See `crates/asperitas-dsp/src/filter.rs` for the existing example of the conventions. Before writing coefficient or smoothing code, name the existing standard formulation or library in the dependency tree that already provides it and record that in the ticket notes, even if the decision is to build it.

Out of scope: more than one voice, excitation shaping, stereo, any host (CLI, live, rig, firmware) wiring.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A voice type implementing Processor is exported from asperitas-dsp, with a ParamSpec table for its parameters and a Default that is a sensible, audible setting
- [ ] #2 Property tests (proptest) show output is finite and bounded for any legal and any sanitised-illegal parameter values, including pitch at and above Nyquist, NaN and zero decay
- [ ] #3 A test shows an impulse rings at the set pitch (measured frequency within a stated tolerance across the range) and decays at the set decay time (measured within a stated tolerance)
- [ ] #4 A test shows silence in gives silence out once the tail has decayed, reset() is idempotent and deterministic, and process_block equals repeated tick
- [ ] #5 A test shows a parameter step produces no output discontinuity above a stated threshold, with a negative control showing an unsmoothed version fails the same test
- [ ] #6 The crate still builds no_std for thumbv7em-none-eabihf (firmware cross-build gate) and performs no allocation
- [ ] #7 Ticket notes name the formulation or library considered for the resonator core and why it was or was not used
- [ ] #8 Documentation: a docs/reference page for the resonator describes the voice, its parameters and units, and is linked from CLAUDE.md's where-things-live list
- [ ] #9 bash scripts/gates.sh push passes
<!-- AC:END -->
