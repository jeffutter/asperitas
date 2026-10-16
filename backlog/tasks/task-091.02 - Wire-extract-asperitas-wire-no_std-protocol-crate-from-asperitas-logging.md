---
id: TASK-091.02
title: 'Wire: extract asperitas-wire (no_std protocol crate) from asperitas-logging'
status: To Do
assignee: []
created_date: '2026-10-16 20:38'
updated_date: '2026-10-16 20:38'
labels:
  - task
dependencies:
  - TASK-091.01
parent_task_id: TASK-091
priority: high
ordinal: 231800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Per owner decision, create crates/asperitas-wire: pure no_std, no embassy deps. Move frame, dump grammar (encoder side), console bodies, capture geometry, excerpt layout/grammar, control. Resolve the coupling: control and usb use inbound::InboundRecord, console/panic_handler use the crate-private TruncWriter (move to wire or give wire its own), frame::Decoder feeds inbound. asperitas-logging keeps runtime (lib, usb, defmt_log, panic_handler, led, spin_budget, inbound) and depends on wire. Move the matching tests/golden files. Repoint firmware, asperitas-rig, asperitas-measure, asperitas-dsp, asperitas-pod, scripts. AC: logging depends on wire not vice versa; all 240 logging tests still pass; firmware builds for every layout; workspace gates pass.
<!-- SECTION:DESCRIPTION:END -->
