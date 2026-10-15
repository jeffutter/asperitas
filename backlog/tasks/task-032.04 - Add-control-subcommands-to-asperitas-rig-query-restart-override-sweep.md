---
id: TASK-032.04
title: 'Add control subcommands to asperitas-rig: query, restart, override sweep'
status: To Do
assignee: []
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 05:59'
labels:
  - task
dependencies:
  - TASK-032.01
parent_task_id: TASK-032
priority: high
ordinal: 158800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the asperitas-rig CLI (crates/asperitas-rig) to send control frames on the same serial device it captures from: query version and reset reason, request a restart and wait for re-enumeration, and hold or sweep a knob override across 0..1 over a timed capture then release. Frames come from the TASK-032 protocol module so host and device share one codec. Host-side unit tests with a fake serial transport; document usage next to the existing rig docs. Out of scope: running on hardware (TASK-033.01).
<!-- SECTION:DESCRIPTION:END -->
