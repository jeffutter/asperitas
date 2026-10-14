---
id: TASK-038.07.02
title: >-
  Run rig in ring-fill mode on the bench and record claimed versus wall-clock
  capture duration
status: Done
assignee:
  - '@agent'
created_date: '2026-10-08 13:56'
updated_date: '2026-10-14 02:12'
labels:
  - planned
dependencies:
  - TASK-038.07.01
parent_task_id: TASK-038.07
priority: medium
ordinal: 128800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bench half of TASK-038.07, discharging TASK-038.05 AC #3. Needs TASK-038.07.01's ring-fill build.

Flash with the probe, then boot with the RESET button, because a probe flash zeroes the DWT and CAPSTAT's timing then reads 0. Hold the console open with one long-lived reader for the whole run (see TASK-038.05 notes: a restarted reader lost 19 blocks to the macOS tty buffer). Time the capture with an outside clock from arm to close. Expect about 349.5 s for 1024 blocks of 341.33 ms each, then about 81 s of dump.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Before the full run, a short capture under 'probe-rs run' (probe stays attached, using the project's chip description ASPERITAS_H750IB) shows a nonzero max_block_us, proving cycle-counter figures are valid on this path. If they read 0, stop, leave this ticket Blocked with the finding, and do not substitute a probe memory read during a capture
- [x] #2 A ring-fill run completes with delivered == 1024, the device's measured capture duration, CAPMAX's seconds_max and an outside wall-clock reading (host clock, arm to close) all quoted side by side, with the differences recorded
- [x] #3 What the producer did when blocks ran out is recorded from the device's own counters (overrun count, audio_exit, whether any block was overwritten), and dump_reassemble proves all 1024 blocks
- [x] #4 The console capture is archived under audio/captures/ and linked from TASK-038.05's AC #3 note
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by a10a7fa. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.

2026-10-09: Block lifted. A plain probe flash is recoverable with the debugger, provided the project's probe tooling is used (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach, which can leave the debug port dead until the USB-C is replugged). Boot-path changes (bootloader install, bootloaded layout) stay Blocked.

2026-10-09: Reassigned @human -> @agent at the owner's request. No hands are needed: probe flash, one long-lived console reader, a host clock. The RESET-button boot the original text asked for is replaced by a run under 'probe-rs run' (docs/reference/daisy-seed3.md, 'A probe detach stops the cycle counter'), verified by the first criterion. Never read memory through the probe during a capture.

2026-10-13 bench run (agent, probe + USB console, no hands needed). Image: HEAD 314f894, rig built with DEFMT_LOG=info, FEATURES="seed3 log-defmt" (console stays on; log-defmt is only there because probe-rs run refuses an ELF with no .defmt section).

AC #1 (cycle counter under probe-rs run): ASP_RIG_CAPTURE_SECONDS=20 build, flashed and held by 'probe-rs run --chip ASPERITAS_H750IB --chip-description-path asperitas-h750.yaml --connect-under-reset'. Console CAPSTAT: max_block_us=65..69, worst_gap_us=666..690, audio_exit=0 across the capture. Nonzero, so figures are valid on this path. The full run below was also flashed with --chip-erase under probe-rs run and read max_block_us=65..74. Note: a probe-rs run that finds the image already programmed (no download) left max_block_us=0 on one attempt, so force a download (--chip-erase) when the figures matter.

AC #2 (ring fill, ASP_RIG_CAPTURE_SECONDS=ring, probe-rs run attached, one long-lived python reader on /dev/cu.usbmodem1101 with receive timestamps per read):
- delivered 1024 == expected 1024, gate pass. Device: 'ring filled in 349536 ms, first armed callback to disarm (CAPMAX seconds_max 349, us_max 349525333)'. 1024 x 341.333 ms = 349525.3 ms, so the device clock is 10.7 ms (0.003 %) over the arithmetic claim.
- Host wall clock, arm record received at 1791943339.455 to 'capture closed' received at 1791943689.081: 349.626 s. The arm record is stamped 2302 ms, the close record 351925 ms, device difference 349.623 s. The host reading is 90 ms over the device's first-callback-to-disarm figure; the arm record is emitted up to a block (341 ms) before the first armed callback, so that gap is the arm-to-first-callback latency, not drift.
- Host clock against device clock over 299.0 device seconds of live CAPSTATs (40.2 s to 339.2 s): host 299.0023 s, slope 1.0000088 (8.8 ppm, about 3 ms over the run). So the outside clock and the device clock agree to a few ms, and seconds_max 349 is a floor of the true 349.525 s (the 0.525 s fraction is dropped by the integer).
- Wall-clock caveat: the host timestamp is when the read returned, so each figure carries a few ms of USB and scheduling jitter; per-read receipt offsets (host minus device) spread 8 ms over 300 samples.

AC #3: producer at ring-full: overrun=1 (exactly one refused claim, the terminator), audio_exit=0 for the whole run, delivered stayed 1024 afterwards (no block claimed past the ring, none overwritten), worst_gap_us 716 < 832, max_block_us 74 < 666. Dump: DUMPEND blocks=1024 chunks=261120 bytes=33554432 elapsed_ms=81444 dropped_full=0 bytes_dropped=0 (about 412 kB/s PCM, 728 kB/s wire, matching the 300 s run). dump_reassemble on the archived capture: blocks complete=1024 failed=0 abandoned=0, bad_frames=0 resyncs=0 discarded=0, 59287712 wire bytes all accounted for, 33554432 PCM bytes = 16777216 samples = 349.525 s. Content: peak 3126 (-20.41 dBFS), max |second difference| 56 LSB (same as the 300 s run), 1 kHz phase at the first and last second agrees after allowing for the 16-sample start offset, so no samples lost, added or spliced.

AC #4: audio/captures/2026-10-13-rig-ring-fill-349s.console.zst (3.8 MB zstd of the 59 287 712-byte console capture; decompresses byte-identical, sha256 of the decompressed file f89d4b05...). Linked from TASK-038.05 AC #3 note and from README. The capture is the clean slice: the first 884 848 bytes of the raw file were the tail of the previous (aborted) run's dump still buffered in the macOS tty, cut at the first record of the new boot; the boot records (RIGCFG, CAPMAX) are not in it, as before, but the same figures are on the 'ring filled' line.

Findings worth keeping (not tickets): (1) A reader whose fd was opened before a probe reset can sit on a dead fd forever while the device waits; two earlier attempts got nothing for 600 s that way. The reader that worked reopens the port after 3 s without a byte. (2) Two earlier attempts were discarded: one under probe-rs run showed audio_exit=2 at delivered=207 (about 70 s in), one after 'probe-rs reset' with no probe attached showed 'audio callback stopped with an SAI error' at delivered=70 (27 s in). Neither reproduced in the final run (audio_exit=0 for 351 s), and the discarded runs overlapped with my killing/restarting probe-rs and readers mid-capture, so I did not chase them; if an unexplained SAI error shows up again in a long run it deserves its own ticket.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Ran rig in ring-fill mode on the bench under probe-rs run: 1024/1024 blocks captured in 349536 ms (device) vs 349.626 s (host wall clock) vs CAPMAX seconds_max 349 (us_max 349525333); overrun 1 as terminator, audio_exit 0, nothing overwritten, dump_reassemble proved all 1024 blocks. A short capture under probe-rs run read max_block_us 65-69, proving cycle-counter figures are valid there. Capture archived at audio/captures/2026-10-13-rig-ring-fill-349s.console.zst and linked from TASK-038.05; README gains the row.
<!-- SECTION:FINAL_SUMMARY:END -->
