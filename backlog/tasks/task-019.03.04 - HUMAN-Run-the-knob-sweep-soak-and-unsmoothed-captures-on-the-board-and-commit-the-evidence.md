---
id: TASK-019.03.04
title: >-
  HUMAN: Run the knob-sweep, soak and unsmoothed captures on the board and
  commit the evidence
status: To Do
assignee:
  - '@human'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 01:01'
labels:
  - task
dependencies:
  - TASK-019.03.01
  - TASK-019.03.02
  - TASK-019.03.03
parent_task_id: TASK-019.03
priority: high
ordinal: 176800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Needs the board and loopback cable. After TASK-019.03.01 to .03 land: (1) flash default rig, replay a named excerpt, sweep each mapped parameter full range with asperitas-rig override, run the sweep analysis (TASK-019.03.02); (2) run 5+ minutes with parameters moving, run soak analysis (.03); (3) flash the unsmoothed build and show soak analysis fails on it; (4) compare device-through-loopback with asperitas-cli at the swept settings on the named excerpt (more excerpts if QSPI budget allows); (5) commit captures and metric summaries and reference them from TASK-019.03; (6) file any disagreement as its own bug ticket rather than loosening a threshold. Boot via the RESET button before timing claims.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Sweep captures for every mapped parameter pass the monotonic, no-flat-region analysis on the board
- [ ] #2 HUMAN: A 5+ minute swept capture passes soak analysis, and the unsmoothed build fails it
- [ ] #3 HUMAN: Device agrees with asperitas-cli within the TASK-035 tolerance on at least one named excerpt
- [ ] #4 HUMAN: Capture artifacts and summaries are committed and referenced from TASK-019.03; disagreements filed as bugs
<!-- AC:END -->
