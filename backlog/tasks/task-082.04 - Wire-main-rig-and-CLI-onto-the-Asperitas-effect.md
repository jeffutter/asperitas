---
id: TASK-082.04
title: 'Wire main, rig and CLI onto the Asperitas effect'
status: Blocked
assignee: []
created_date: '2026-10-16 22:35'
updated_date: '2026-10-17 00:38'
labels:
  - task
  - planned
dependencies:
  - TASK-082.04.03
parent_task_id: TASK-082
priority: high
ordinal: 237800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace KnobChain's hand-wired filter/gain and knob mapping in firmware/src/bin/main.rs with the Asperitas effect; make rig (replay_fx.rs/fx_chain.rs, FxKind::from_code) run the same chain, shrinking ReplayFx/FxKind to bypass-or-patch or removing them; make asperitas-cli process build from the same parameter table. Acceptance: knob mapping lives only in asperitas-dsp; main and rig run the same chain; no triple buffering in main.rs; all variants build and host gate passes; max_block_us recheck is deferred to the hardware recheck ticket TASK-120 (HUMAN).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against a7eee18
Approach: Asperitas (asperitas-dsp) already owns knob mapping and the filter-then-gain chain; this ticket swaps its three consumers onto it. Split into three independent sub-tickets: .01 firmware main (replace KnobChain), .02 asperitas-cli process (table/Asperitas driven), .03 rig + FxChain/ReplayFx + asperitas-measure + wire docs (the widest; depends on .01 so the rig mirrors main's chain). Order: .01, .02, .03. Final check: grep that params_from_normalised/knob mapping exists only in asperitas-dsp; host gate and all firmware variants build; max_block_us recheck belongs to TASK-120 (HUMAN). Risk: rig wire semantics change (CTL FX) touches measure and docs; keep bypass bit-exact.
<!-- SECTION:PLAN:END -->
