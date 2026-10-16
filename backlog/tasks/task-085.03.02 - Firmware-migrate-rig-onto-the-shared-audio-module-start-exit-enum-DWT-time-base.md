---
id: TASK-085.03.02
title: >-
  Firmware: migrate rig onto the shared audio module (start(), exit enum, DWT
  time base)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:20'
updated_date: '2026-10-16 20:35'
labels:
  - task
  - planned
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 9c873c2. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
rig now implements audio::AudioProcessor (RigAudio in a StaticCell) and calls audio::start; rig's own executor, SAI1 vector, encode_block, audio_task and start_interface block are gone. capture_status reads audio::AUDIO_EXIT (0 running, 1 NotATransmitter, 2 NotAReceiver, 3 Overrun, |0x10 start failed); docs and .cargo/config.toml updated from 2 to 3 for overrun. TimeBase, measure_time_base and trace_enabled moved into audio.rs as pub items (log text made binary-neutral). ledtest unaffected. Verified by builds of every rig variant (bootloaded matrix), the other binaries, RTT-only config, and clippy -D warnings. Hardware behaviour left to TASK-120.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig runs on the shared audio module: one encode_block, one executor/vector, one AUDIO_EXIT encoding, shared DWT time base. Builds and clippy pass; on-device checks are TASK-120.
<!-- SECTION:FINAL_SUMMARY:END -->
