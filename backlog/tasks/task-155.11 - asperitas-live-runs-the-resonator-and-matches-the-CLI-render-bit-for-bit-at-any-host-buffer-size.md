---
id: TASK-155.11
title: >-
  asperitas-live runs the resonator, and matches the CLI render bit for bit at
  any host buffer size
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - live
dependencies:
  - TASK-155.06
documentation:
  - docs/reference/asperitas-live.md
  - crates/asperitas-live/tests/equivalence_cli.rs
  - crates/asperitas-live/tests/equivalence_automation.rs
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 306700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
M4 made `asperitas-live` the way to iterate on sound without a flash cycle, and its exit criterion was that the live engine and the offline render agree exactly given the same controls and input. The resonator needs the same: a way to run it live (starting params, knob view, recording) and the same equivalence guarantee, so what is heard live is what the CLI and the device compute.

Add the resonator as a selectable effect in `asperitas-live`, with starting values through the same `--params key=value` syntax and the resonator controls (TASK-155.05), a knob view that shows the resonator's controls in normalised and real units, and `--record` working with it. Controls latch only on the 32-frame grid counted from stream start, whatever buffer size the OS hands it. The default effect with no selector is unchanged. Equivalence tests follow `crates/asperitas-live/tests/equivalence_cli.rs` and `equivalence_automation.rs`.

Out of scope: Pod-specific encoder or button handling beyond what the keyboard view needs; line-in hardware not present on the dev machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 asperitas-live selects the resonator with a documented flag, takes --params in its real-unit keys, and runs it on a WAV or line-in; with no selector the existing patch runs unchanged and existing live tests pass
- [ ] #2 The knob view shows the resonator's controls in normalised and real units and moves them in the documented steps; with no terminal the view is disabled as for the existing effect
- [ ] #3 A test shows the live engine's output equals asperitas-cli process for the resonator bit for bit at several host buffer sizes, including sizes not a multiple of 32, with a negative control that latches at the wrong grid and differs
- [ ] #4 A test shows the same equivalence with an automation file driving the resonator's controls
- [ ] #5 The audio-callback path for the resonator performs no allocation, covered the way crates/asperitas-live/tests/no_alloc.rs covers the existing effect
- [ ] #6 docs/reference/asperitas-live.md documents the selector, keys, knob view for the resonator and the equivalence guarantee
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
