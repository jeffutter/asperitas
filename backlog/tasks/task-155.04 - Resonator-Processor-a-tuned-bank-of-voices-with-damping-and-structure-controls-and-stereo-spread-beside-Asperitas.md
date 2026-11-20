---
id: TASK-155.04
title: >-
  Resonator Processor: a tuned bank of voices with damping and structure
  controls and stereo spread, beside Asperitas
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - dsp
dependencies:
  - TASK-155.01
  - TASK-155.02
  - TASK-155.03
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - crates/asperitas-dsp/src/asperitas.rs
  - crates/asperitas-dsp/src/processor.rs
parent_task_id: TASK-155
priority: high
type: feature
ordinal: 299700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
This is the effect itself: a `Processor` in `asperitas-dsp` that runs the excitation stage into a bank of tuned resonator voices and spreads the result across the stereo frame, so a plucked or bowed note makes the bank ring sympathetically. It is added beside `Asperitas`, which stays unchanged, with its own `Params` type and parameter table, and the same no_std, no allocation, no `Result`, smoothing-inside, sanitised-inputs conventions.

It composes the single voice (TASK-155.02) and the excitation stage (TASK-155.03) and implements the tuning model, voice count and default voicing the owner chose in TASK-155.01 (read the decision record under backlog/decisions; if it is not there, this ticket is not ready). Damping (how fast the bank decays and how bright the tail stays) and structure (how the voices are related, per the chosen tuning model) are real-unit parameters in the `ParamSpec` table. Stereo spread places voices across left and right; the input is mono and both input channels are summed.

Out of scope: the knob-position `Controls` mapping (TASK-155.05), any host wiring, modulation (M6).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Resonator type implementing Processor with its own Params and a ParamSpec table is exported from asperitas-dsp and Asperitas is unchanged, with every existing golden still passing
- [ ] #2 The tuning model, voice count and default voicing match the decision record from TASK-155.01, and the record is cited in the rustdoc
- [ ] #3 Property tests (proptest) show output is finite and bounded for any bounded input and any legal or sanitised-illegal parameters, including the maximum voice count at maximum feedback or decay
- [ ] #4 A test shows exciting the bank with an impulse makes it ring at the tuned pitches (measured peaks match the tuning within a stated tolerance) and the tail decays according to the damping parameter in the stated direction
- [ ] #5 A test shows stereo spread: at zero spread both output channels are identical, and at full spread the measured left/right energy differs as documented, with a negative control
- [ ] #6 Silence in gives silence out once tails decay, reset() is idempotent and deterministic, process_block equals repeated tick, and a parameter step produces no output discontinuity above a stated threshold
- [ ] #7 A measured per-block cost of the bank at the maximum voice count is recorded in the ticket notes, in the same unit the project uses for firmware headroom, so the voice budget has a number behind it
- [ ] #8 The crate still builds no_std for thumbv7em-none-eabihf and performs no allocation
- [ ] #9 The resonator docs/reference page documents the bank, its parameters, units and the tuning model, and is linked from CLAUDE.md
- [ ] #10 bash scripts/gates.sh push passes
<!-- AC:END -->
