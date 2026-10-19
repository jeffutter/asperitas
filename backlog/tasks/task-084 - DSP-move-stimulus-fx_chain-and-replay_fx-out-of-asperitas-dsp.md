---
id: TASK-084
title: 'DSP: move stimulus, fx_chain and replay_fx out of asperitas-dsp'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 17:20'
labels:
  - review-2026-10
  - dsp
  - architecture
  - planned
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
- [x] #1 asperitas-dsp contains no wire codes or measurement stimuli
- [x] #2 The CLI sweep and rig sweep share one implementation
- [x] #3 .text of the rig images is re-measured and recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit that carries this ticket's Task-Id trailer. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner decision 2026-10-16: move them to keep the DSP crate clean.

Created crates/asperitas-stimulus (stimulus, fx_chain, replay_fx; depends on dsp and asperitas-wire). DescWriter replaced by asperitas-wire BodyWriter (identical semantics). The sweep phase expression now lives once in stimulus::Chirp; ExponentialSweep (fades + level scale) and the CLI golden sweep both use it. CLI sweep length is now round(fs*secs) in f64 instead of f32 truncation; goldens unchanged at 48 kHz x 1 s. stimulus_shape_tests.rs removed (it guarded the duplication); a Chirp-vs-sweep-interior test replaces it in asperitas-stimulus. Proptest stimulus section split into tests/stimulus_properties.rs. .text (rust-size) before -> after: rig bootloaded 158589 -> 158501, rig stim-excerpt bootloaded 158909 -> 158893, main internal 109773 -> 109773.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved stimulus, fx_chain and replay_fx into the new asperitas-stimulus crate; asperitas-dsp now holds no wire codes or stimuli. CLI and device sweeps share stimulus::Chirp. Rig .text: 158589 -> 158501 bytes (default sine), 158909 -> 158893 (stim-excerpt); main unchanged at 109773. gates.sh push passes.
<!-- SECTION:FINAL_SUMMARY:END -->
