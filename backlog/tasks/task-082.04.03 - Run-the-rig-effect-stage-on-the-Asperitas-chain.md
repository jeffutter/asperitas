---
id: TASK-082.04.03
title: Run the rig effect stage on the Asperitas chain
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 00:38'
updated_date: '2026-10-17 00:53'
labels:
  - task
  - planned
dependencies:
  - TASK-082.04.01
parent_task_id: TASK-082.04
priority: high
ordinal: 240800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rig (firmware/src/bin/rig.rs fx_ctl, asperitas-dsp fx_chain.rs/replay_fx.rs) selects a ReplayFx via CTL FX (bypass/gain/lowpass, FxKind::from_code) with a single KnobOverride<1>. Make the rig run the same Asperitas chain as main: shrink ReplayFx/FxKind to bypass-or-patch (or remove them), update FxChain, rig fx_ctl, asperitas-measure compare.rs (FxSetting, ReplayFx references), asperitas-wire CTL FX docs/semantics, and docs/reference/asperitas-rig.md consistently. Decide the wire shape (e.g. FX f=00 bypass, f=01 patch with SET k=00/01 overriding both knobs via KnobOverride<2>) and keep bypass bit-exact. Acceptance: rig and main run the same chain; no remaining knob mapping outside asperitas-dsp; all variants build; host gate passes; hardware recheck deferred to TASK-120 (HUMAN).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Rig fx_ctl and main both run asperitas_dsp Asperitas; ReplayFx/FxKind reduced to bypass-or-patch
- [x] #2 CTL FX f=00 bypass stays bit-exact; f=01 patch; SET/REL k=00/01 override cutoff/gain via KnobOverride<2>
- [x] #3 asperitas-measure compare, asperitas-wire docs and docs/reference/asperitas-rig.md updated consistently
- [x] #4 No knob mapping outside asperitas-dsp; all firmware variants build; host gate passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 613e321. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
FxKind reduced to {Bypass, Patch}; ReplayFx wraps asperitas_dsp::Asperitas (added Asperitas::snap_params for the unsmoothed proof build). Wire: FX f=00 bypass, f=01 patch at Controls::default knobs, v accepted and ignored; SET/REL k=00 cutoff, k=01 gain via KnobOverride<2> (KNOB_COUNT) merged over defaults each block. FxChain::process_block_with closure now maps [f32;2]. asperitas-measure: FxSetting {code, knobs:[Option<u16>;2]}, --fx bypass|patch plus repeatable --fx-knob K=V replaces --fx-value; soak chain test settles the gain knob before capture. scripts/rig-session.py gets a KNOB env. docs/reference/asperitas-rig.md rewritten; earlier gain/lowpass measurements marked as taken on the earlier build. Gates: scripts/gates.sh commit and push tiers pass (all rig variants build, workspace tests pass). No knob mapping outside asperitas-dsp apart from asperitas-pod test usage of the dsp API. Hardware recheck deferred to TASK-120 (HUMAN); no audio verification claimed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rig effect stage is now bypass-or-Asperitas-patch, sharing the chain main runs; CTL FX f=00/01, SET/REL knobs 0 (cutoff) and 1 (gain) via KnobOverride<2>; asperitas-measure compare/CLI, wire docs, rig-session script and docs/reference/asperitas-rig.md updated. Bypass stays bit-exact (tests). Host gate and all firmware variants build.
<!-- SECTION:FINAL_SUMMARY:END -->
