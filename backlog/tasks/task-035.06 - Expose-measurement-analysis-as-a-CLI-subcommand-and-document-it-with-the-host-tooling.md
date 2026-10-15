---
id: TASK-035.06
title: >-
  Expose measurement analysis as a CLI subcommand and document it with the host
  tooling
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 19:57'
updated_date: '2026-10-15 19:57'
labels:
  - task
dependencies:
  - TASK-035.02
  - TASK-035.03
  - TASK-035.04
  - TASK-035.05
parent_task_id: TASK-035
priority: high
ordinal: 167800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an analyze entry point (asperitas-measure binary or asperitas-rig subcommand) that prints metrics JSON and exits 0 pass / 1 failed verdict / 2 error, matching asperitas-rig convention. Document usage, metric definitions, blind spots, and fixture workflow in docs/reference/asperitas-rig.md and the README measurement-rig section. Covers TASK-035 AC7. Depends on all other subtasks.
<!-- SECTION:DESCRIPTION:END -->
