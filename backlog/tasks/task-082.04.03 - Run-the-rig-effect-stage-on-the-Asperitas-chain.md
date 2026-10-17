---
id: TASK-082.04.03
title: Run the rig effect stage on the Asperitas chain
status: To Do
assignee: []
created_date: '2026-10-17 00:38'
updated_date: '2026-10-17 00:38'
labels:
  - task
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
