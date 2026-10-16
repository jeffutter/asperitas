---
id: TASK-085
title: >-
  Firmware: add a package lib with one audio module, shared panic/logger
  preamble and host-control glue
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - firmware
  - architecture
dependencies: []
priority: high
type: chore
ordinal: 187800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The firmware has no lib target, so boilerplate is copied per binary and kept in line by comments and grep gates. encode_block is copied word for word (main.rs:137-144, rig.rs:281-292); the SAI1 InterruptExecutor, vector, P6 priority and spawn appear twice (main.rs:176-184,366-372; rig.rs:261-279,2361-2366); prepare_interface/start_interface handling appears twice with different failure policies (main spins in thread mode and kills the console on a start failure, main.rs:340-350, the TASK-076 failure mode; rig handles it, rig.rs:2317-2342). AUDIO_EXIT has two encodings (main.rs:146-162 vs rig.rs:600). Six binaries each repeat a ~50-line panic_handler/_defmt_panic/no-op-logger preamble justified by comments that main.rs:56 itself disproves (defmt_rtt provides a global_logger from a library). main.rs:8 and podtest.rs:6 use #[path] to share host_control.rs, with a blanket allow(dead_code). The DWT time base (rig.rs:2065-2195) will be wanted by main once effects get heavier.

Change: firmware/src/lib.rs with a deep audio module (codec encode/decode, SAI1 interrupt executor + priority, one start(interface, callback) entry point with one failure policy that logs, sets the LED and keeps serving the console, one exit-code enum, DWT time base), the shared panic/logger preamble linked with 'use asperitas_firmware as _;', and host_control as a lib module. Do a link experiment for the preamble first. ledtest may opt out of the USB/SAI handlers.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 main and rig both start audio through the same audio::start; one AUDIO_EXIT encoding
- [ ] #2 encode_block and the SAI1 executor setup exist once
- [ ] #3 The per-binary panic/logger preamble is replaced by the lib, with panictest still reaching LED and USB (checked in the hardware recheck ticket)
- [ ] #4 #[path] imports and the blanket allow(dead_code) are gone
- [ ] #5 scripts/check-main-audio-executor.sh is deleted or reduced because the guarantee holds by construction
<!-- AC:END -->
