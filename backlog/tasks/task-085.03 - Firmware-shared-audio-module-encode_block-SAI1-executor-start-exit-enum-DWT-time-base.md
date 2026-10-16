---
id: TASK-085.03
title: >-
  Firmware: shared audio module (encode_block, SAI1 executor, start(), exit
  enum, DWT time base)
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-16 20:00'
updated_date: '2026-10-16 20:20'
labels:
  - task
  - firmware
  - planned
dependencies:
  - TASK-085.03.01
  - TASK-085.03.02
parent_task_id: TASK-085
priority: high
ordinal: 226800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add firmware audio module to the lib: codec encode/decode (encode_block exists once), the SAI1 InterruptExecutor + vector + P6 priority + spawn, one start(interface, callback) entry with a single failure policy (log, set LED, keep serving the console - not main's current spin that kills the console, the TASK-076 failure mode), one AUDIO_EXIT enum replacing the two encodings (main.rs vs rig.rs), and the DWT time base from rig.rs. Migrate main and rig onto it; ledtest may opt out of the USB/SAI handlers. Delete or reduce scripts/check-main-audio-executor.sh since the guarantee holds by construction, and update any gate that referenced it. Depends on the lib target ticket. Acceptance: main and rig both call audio::start; encode_block and the executor setup exist once; one AUDIO_EXIT encoding; script deleted or reduced; builds and gates pass; hardware behavior checked in TASK-120.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against e5e8afd
Tracking ticket. Work is split: .01 builds the lib audio module and migrates main (and removes the check-main-audio-executor script and gate), .02 migrates rig onto it. Done when both are Done, gates pass, and hardware recheck proceeds in TASK-120.
<!-- SECTION:PLAN:END -->
