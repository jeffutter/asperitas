---
id: TASK-082.04.02
title: Build asperitas-cli process from the Asperitas effect and parameter tables
status: To Do
assignee: []
created_date: '2026-10-17 00:38'
labels:
  - task
dependencies: []
parent_task_id: TASK-082.04
priority: medium
ordinal: 239800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-cli/src/process.rs hand-matches 'gain' and 'filter' and builds each processor from its param table. Make it derive from the shared tables/Asperitas effect: keep per-stage processors selectable if useful, and add the full chain (e.g. processor 'asperitas' taking cutoff_hz and gain_db) built via Asperitas::new / Controls, with no duplicate table-walking logic. Acceptance: process supports the full effect chain; parameter parsing stays table-driven; existing cli tests pass or are updated; host gate passes.
<!-- SECTION:DESCRIPTION:END -->
