---
id: TASK-082.04
title: 'Wire main, rig and CLI onto the Asperitas effect'
status: To Do
assignee: []
created_date: '2026-10-16 22:35'
updated_date: '2026-10-16 22:35'
labels:
  - task
dependencies:
  - TASK-082.03
parent_task_id: TASK-082
priority: high
ordinal: 237800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace KnobChain's hand-wired filter/gain and knob mapping in firmware/src/bin/main.rs with the Asperitas effect; make rig (replay_fx.rs/fx_chain.rs, FxKind::from_code) run the same chain, shrinking ReplayFx/FxKind to bypass-or-patch or removing them; make asperitas-cli process build from the same parameter table. Acceptance: knob mapping lives only in asperitas-dsp; main and rig run the same chain; no triple buffering in main.rs; all variants build and host gate passes; max_block_us recheck is deferred to the hardware recheck ticket TASK-120 (HUMAN).
<!-- SECTION:DESCRIPTION:END -->
