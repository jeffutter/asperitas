---
id: TASK-084
title: 'DSP: move stimulus, fx_chain and replay_fx out of asperitas-dsp'
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - dsp
  - architecture
  - needs-decision
dependencies:
  - TASK-082
priority: low
type: chore
ordinal: 186800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
asperitas-dsp holds rig and measurement plumbing: stimulus.rs (~900 lines, with a wire-format writer and a DescWriter duplicated from logging, stimulus.rs:206-240), fx_chain.rs, replay_fx.rs (u8/u16 wire codes, ' fx=' describe). The CLI's sweep re-implements ExponentialSweep with float-truncated length (synth.rs:37-57 vs stimulus.rs:491) and stimulus_shape_tests.rs guards the duplication rather than removing it.

Change: move them to an asperitas-stimulus (or rig-side) crate that depends on dsp so dsp stays the effect core.
DECISION NEEDED: is one shared crate for firmware/host bit-identity worth the coupling? Rig builds are flash-size sensitive, so re-measure .text.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 asperitas-dsp contains no wire codes or measurement stimuli
- [ ] #2 The CLI sweep and rig sweep share one implementation
- [ ] #3 .text of the rig images is re-measured and recorded
<!-- AC:END -->
