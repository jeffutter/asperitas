---
id: TASK-085.03.02
title: >-
  Firmware: migrate rig onto the shared audio module (start(), exit enum, DWT
  time base)
status: To Do
assignee: []
created_date: '2026-10-16 20:20'
updated_date: '2026-10-16 20:20'
labels:
  - task
dependencies:
  - TASK-085.03.01
parent_task_id: TASK-085.03
priority: high
ordinal: 229800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Move rig.rs's encode_block, SAI1 executor/vector, AUDIO_EXIT encoding onto the lib audio module; the DWT time base from rig.rs moves into the audio module so main and rig share it. ledtest may opt out of the USB/SAI handlers. Acceptance: rig calls audio::start; no duplicate encode_block/executor in rig; one AUDIO_EXIT encoding with rig's console reporting updated to match; builds and gates pass; hardware behaviour is checked in TASK-120.
<!-- SECTION:DESCRIPTION:END -->
