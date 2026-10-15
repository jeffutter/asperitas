---
id: TASK-035.07
title: >-
  Run a processor on the rig's device replay path and compare against
  asperitas-cli at non-identity parameters
status: Done
assignee:
  - '@human'
created_date: '2026-10-15 20:37'
updated_date: '2026-10-15 23:49'
labels:
  - task
  - planned
dependencies:
  - TASK-035.07.01
  - TASK-035.07.02
  - TASK-035.07.03
parent_task_id: TASK-035
priority: medium
ordinal: 169800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up from TASK-035.05. firmware/src/bin/rig.rs replays QSPI excerpts straight to the DAC with no asperitas-dsp processor in the path (its Processor use is the stimulus generators only), so TASK-035.05 can only compare at identity. Add a processor (e.g. Gain, then filter) to the replay path behind a rig control, and extend the 035.05 comparison to non-identity parameters. Firmware change and on-device capture; the capture step needs the board so split before execution (HUMAN capture subtask).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All subtasks (035.07.01-03) are done
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 496e4ad
Split into: .01 firmware replay processor + rig control (agent), .02 host comparison at non-identity params (agent, synthetic captures), .03 HUMAN on-device capture. Order: .01 and .02 in parallel, then .03. Parent is @human by inheritance from .03. No direct work remains in the parent.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15: closed with the owner. All three subtasks Done: .01 replay effect and CTL FX control (b49cfd6), .02 comparison at non-identity settings, .03 on-board capture at three settings, all passing (e99ab58), signed off by the owner.
<!-- SECTION:NOTES:END -->
