---
id: TASK-091.03
title: 'Host: create asperitas-host std crate for host-only decode and assembly'
status: To Do
assignee: []
created_date: '2026-10-16 20:38'
updated_date: '2026-10-16 20:38'
labels:
  - task
dependencies:
  - TASK-091.02
parent_task_id: TASK-091
priority: high
ordinal: 232800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Per owner decision, create crates/asperitas-host (std): BlockAssembler, dump::decode, parse_wav, install_frames, the dump_reassemble binary (from logging examples), and shared capture loading currently duplicated in asperitas-rig/asperitas-measure. Wire stays no_std and loses these items. Depends on the wire extraction. AC: no no_std/alloc-free constraint remains on host-only code; rig and measure depend on host and wire only; tests moved and passing.
<!-- SECTION:DESCRIPTION:END -->
