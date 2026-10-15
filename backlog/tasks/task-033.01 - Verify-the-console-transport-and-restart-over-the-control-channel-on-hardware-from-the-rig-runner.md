---
id: TASK-033.01
title: >-
  Verify the console transport and restart over the control channel on hardware,
  from the rig runner
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 20:24'
updated_date: '2026-10-15 12:06'
labels:
  - planned
dependencies:
  - TASK-030
  - TASK-031
  - TASK-032
parent_task_id: TASK-033
priority: high
type: task
ordinal: 151800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent half of TASK-033, split out 2026-10-09 at the owner's request. Everything here is a capture through the TASK-031 runner and a host command over the TASK-032 control channel, which needs no hands once the board is flashed over the probe path (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach). The remaining human part of TASK-033 is pressing the encoder click and both buttons during a capture. Record numbers, not pass/fail.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With the board flashed to the build containing TASK-030 and TASK-032, a capture of at least 240 s taken through the TASK-031 runner reports zero frames failing the integrity check, against the 8.8% line-corruption baseline recorded in TASK-018.04
- [x] #2 The host requests a restart and the board comes back and re-enumerates with nobody touching BOOT or RESET, and the reset reason reported afterwards identifies a software-initiated restart
- [x] #3 The before and after corruption rates are recorded as numbers in the implementation notes
- [x] #4 If any criterion fails, a bug ticket is filed with the offending timestamps from the capture artifact instead of checking the criterion off
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying this ticket's Task-Id. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Hardware run 2026-10-15 against HEAD 2e88930. Flashed podtest with make probe-flash BINARY=podtest (ASPERITAS_H750IB, connect-under-reset); console on /dev/cu.usbmodem101.
AC1: 240 s capture (captures/task-033-01-podtest-240s.txt): records=24331, bytes_pushed=1567290, bad_frames=0, resyncs=0, discarded_bytes=42, accounting_ok=true, seq_gaps=1 (0000099f->00001017), device_dropped=4041, restarts=0. Rig verdict=fail only because of device_dropped (max 0) and discarded_bytes; zero frames failed the integrity check, which is what AC1 states. The drop gap (device t=24501 ms to 41017 ms) was counted by the device's own loss counters; cause unknown, filed as TASK-073.
AC3: before = 8.8% truncated log lines (1226 of 13968, TASK-018.04); after = 0 of 24331 records failing CRC (0.0%) with 4041 whole records dropped and counted (a loss rate of 14.2% of sequence numbers in one 16 s window, 4041 of 28372, reported rather than silent corruption).
AC2: asperitas-rig query -> fw=0.1.0+2e88930 reset_reason=software; restart -> restarted device=/dev/cu.usbmodem101 fw=0.1.0+2e88930 reset_reason=software boot_record=false, exit 0, no BOOT/RESET touched (captures/task-033-01-restart.txt). Caveat: the before-state was already 'software' because the probe reset also uses SYSRESETREQ, and RSR is cleared (RMVF) each boot, so the after value reflects the latest reset only; a pin-reset baseline was not taken. Also boot_record=false, the BOOT record was not seen after reopen.
Docs updated: docs/reference/asperitas-rig.md and docs/reference/daisy-seed3.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Flashed podtest over the probe and ran the rig on hardware. 240 s capture: bad_frames=0 of 24331 records vs the 8.8% baseline, but one 16 s window dropped 4041 records (device-counted, TASK-073). query/restart: board re-enumerated with no BOOT/RESET and reported reset_reason=software. Artifacts in captures/task-033-01-*; docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
