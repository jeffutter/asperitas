---
id: TASK-082.03
title: 'DSP: top-level Asperitas effect owning controls, chain and table'
status: To Do
assignee: []
created_date: '2026-10-16 22:35'
updated_date: '2026-10-16 22:35'
labels:
  - task
dependencies:
  - TASK-082.01
  - TASK-082.02
parent_task_id: TASK-082
priority: high
ordinal: 236800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an Asperitas processor in asperitas-dsp with set_controls(&Controls{knobs:[f32;N],..}) that owns the filter-then-gain chain, maps controls to parameters via the parameter tables, and processes blocks in place. No soft-clip stage (owner decision: hard clamp at codec boundary stays). Host tests: chain equals the manual filter->gain composition bit-for-bit for sample knob settings, NaN/inf robustness, determinism after reset. Depends on the in-place block and parameter-table subtasks. No generic plugin graph.
<!-- SECTION:DESCRIPTION:END -->
