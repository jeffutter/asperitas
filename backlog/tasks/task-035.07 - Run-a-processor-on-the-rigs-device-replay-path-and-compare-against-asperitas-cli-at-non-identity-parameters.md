---
id: TASK-035.07
title: >-
  Run a processor on the rig's device replay path and compare against
  asperitas-cli at non-identity parameters
status: To Do
assignee: []
created_date: '2026-10-15 20:37'
labels:
  - task
dependencies: []
parent_task_id: TASK-035
priority: medium
ordinal: 169800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up from TASK-035.05. firmware/src/bin/rig.rs replays QSPI excerpts straight to the DAC with no asperitas-dsp processor in the path (its Processor use is the stimulus generators only), so TASK-035.05 can only compare at identity. Add a processor (e.g. Gain, then filter) to the replay path behind a rig control, and extend the 035.05 comparison to non-identity parameters. Firmware change and on-device capture; the capture step needs the board so split before execution (HUMAN capture subtask).
<!-- SECTION:DESCRIPTION:END -->
