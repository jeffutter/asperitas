---
id: TASK-035.05
title: >-
  Compare device-through-loopback output with asperitas-cli over the same QSPI
  excerpt, with documented tolerance
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
parent_task_id: TASK-035
priority: high
ordinal: 166800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Compare a capture of an excerpt replayed through the loop (EXCPLAY captures, e.g. 2026-10-15-rig-excerpt-replay-mand-soft) with asperitas-cli process output over the matching audio/goldens/excerpt WAV at matched parameters, after alignment and loop-gain normalization. Tolerance is stated in the thresholds module and the reason bit-exactness is not claimed (two converters, analog noise floor ~16-bit, loop gain, DC offset) is written next to it. RISK: verify whether rig.rs currently runs a processor on the device path; if only passthrough, compare at identity/gain parameters and file a follow-up for processor-on-device. Covers TASK-035 AC4. Depends on alignment and metrics.
<!-- SECTION:DESCRIPTION:END -->
