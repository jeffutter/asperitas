---
id: TASK-030.06
title: >-
  Capture the framed console from podtest at idle on hardware: zero bad frames,
  counters, complete trailing record
status: Done
assignee:
  - '@agent'
created_date: '2026-10-09 11:00'
updated_date: '2026-10-09 11:21'
labels:
  - planned
dependencies: []
parent_task_id: TASK-030
priority: high
ordinal: 144800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent half of TASK-030.04, split out 2026-10-09 because everything here is flash, capture, decode and record numbers, which the probe path does without hands. The hands-and-ears half (encoder clicks, button presses, audio listening) stays in TASK-030.04, which now depends on this ticket.

Protocol: flash the build containing TASK-030.02 with the project's probe path (make probe-flash, chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach, which can leave the debug port dead until the USB-C is replugged). Tee the console bytes to a file and decode with `cargo run -p asperitas-logging --example console_decode -- capture.raw` (TASK-030.01). Nothing is pressed or turned during the capture, so a missing control event proves nothing here. Record numbers, not pass/fail claims. If something fails, file a bug ticket with the offending timestamps instead of checking a box.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Flash podtest (and one of blinky or panictest) built with TASK-030.02 over the probe path. A plain terminal read of the console shows podtest knob lines still readable with the new prefix and CRC suffix, and the boot messages appear normally
- [x] #2 A raw capture of at least 60 s of podtest at idle is decoded with console_decode and reports zero records failing the integrity check
- [x] #3 Decoded record count, achieved records per second, the device-reported drop counters from the STATUS records and the host-side integrity failures are recorded as numbers in the notes
- [x] #4 The raw file ends on a complete record rather than a truncated tail, and the byte length of the last record modulo 64 is noted
- [x] #5 If any criterion fails, a bug ticket carrying the offending timestamps is filed instead of checking the box
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this ticket's commit (measurement-only; no source changes). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measured on hardware via probe path (make probe-flash BINARY=podtest, chip ASPERITAS_H750IB, console /dev/cu.usbmodem1101). AC1: podtest console read with plain cat shows BOOT proto=1 fw=0.1.0 and readable "[podtest] t=.. r1=.. r2=.." lines with "~I <seq> <ms>" prefix and "*crc" suffix. blinky also flashed and read: BOOT, "Blinky running", "USB connected", STATUS. AC2/3 capture cap4 (75 s idle, 490841 bytes): console_decode records=7634 bad_frames=0 resyncs=0 discarded_bytes=0, bytes pushed=accounted_for=490841. Seq gaps=1: 00000025->00001d46, which is the host attaching 74 s after reset (device pipe full, dropping before any reader); the CRC check cannot see absence, the STATUS counters confirm it. Steady window after attach: 7596 records in 74.99 s = 101.3 records/s, zero gaps. 75 STATUS records: first dropped_full=7456 bytes_dropped=468147 trunc=0 ep_err=0 pipe_free=2048 (seq 1d47, t=74.69 s); last identical dropped_full=7456 bytes_dropped=468147 trunc=0 ep_err=0 (seq 3ac3, t=149.2 s), sent 39->7587. So zero drops while a reader was attached. Host-side integrity failures: 0. An earlier capture (85 s, board already running) likewise gave 7631 records, bad_frames=0. AC4: file ends on complete record ending CRLF; last record 64 bytes (including CRLF), 64 mod 64 = 0. AC5: no criterion failed, so no bug ticket filed. Observation: a late-attaching host loses the boot-time records (BOOT survives only when a reader is attached at boot), expected by design.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Flashed podtest (and blinky) over the probe path and captured the framed console at idle: 75 s, 7634 records, 0 bad frames, 0 resyncs, 101.3 rec/s in steady state, device drop counters constant (dropped_full=7456 all from before the host attached), file ends on a complete 64-byte record. Details in notes.
<!-- SECTION:FINAL_SUMMARY:END -->
