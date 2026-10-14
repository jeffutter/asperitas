---
id: TASK-059.04
title: >-
  Check the relocated SAI DMA buffers digitally: blinky boot, rig stimulus
  through the loop, counters across resets
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 20:24'
updated_date: '2026-10-14 02:21'
labels:
  - planned
dependencies:
  - TASK-059.01
parent_task_id: TASK-059
priority: medium
type: task
ordinal: 150800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent half of TASK-059.03, split out 2026-10-09 at the owner's request. TASK-059.03 assumed no objective loopback exists; TASK-038.05 has since measured the Pod loop at -0.41 dB at -20 dBFS (docs/reference/daisy-pod.md), so a stimulus through the loop is digital evidence that the SAI DMA buffers at 0x240021b8..0x240025b8 work. What stays in TASK-059.03 is what only a person can judge: the steady-green LED and hearing main. Use the project's probe path only (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach). Do not read memory through the probe during a capture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 blinky built from the post-TASK-059.01 tree is flashed over the probe path; the first four bytes of the flashed image are recorded and the board is confirmed alive by a probe-rs attach that reads the vector table and finds a plausible reset handler (an agent cannot see the LED, so the steady-green check stays in TASK-059.03)
- [x] #2 rig is flashed with FEATURES=seed3 and its stimulus returns through the loop: the console shows BOOT, one RIGCFG record reading icache=0 dcache=0, and STATUS records whose sent= advances with dropped_full=0; the actual counter values are pasted, decoded with console_decode if captured to a file
- [x] #3 The returned level through the loop matches the expected -0.41 dB at -20 dBFS within a stated tolerance, so the DMA buffers are demonstrably moving samples and not stale data
- [x] #4 The previous two criteria are repeated across at least two resets or power cycles reached by the probe, and the results are quoted side by side
- [x] #5 Anything unexpected is filed as a bug ticket carrying the counters or capture position, not checked off
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 7ee0ffc. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-13 bench run (agent, probe + USB console), HEAD 3fe6064, post-TASK-059.01 tree, project chip description only (ASPERITAS_H750IB, --connect-under-reset).

AC1: blinky built with 'make build BINARY=blinky FEATURES="seed3 log-defmt"' and flashed with make probe-flash. blinky.bin first four bytes: 00 00 08 24 (initial SP 0x24080000, end of the 512K AXI SRAM). Probe read of the flashed vector table at 0x08000000: 24080000 08000299 0800ec81 08010b31; reset handler 0x08000299 is odd (Thumb) and inside the 81256-byte image (0x08000000..0x08013d68); the bin's word 1 (99 02 00 08) matches flash. A probe-rs attach to the ELF refused it (blinky has no .defmt section), so liveness is shown by the vector table read plus a clean probe reset and re-attach, not by a running-state read (DHCSR read under reset shows halted, which says nothing). Steady green LED stays in TASK-059.03.

AC2/AC4: rig built ASP_RIG_CAPTURE_SECONDS=10, FEATURES=seed3 (console only), probe-flashed, then two 'probe-rs reset --connect-under-reset' resets, each with one long-lived reader (reader reopens port after 3 s silence) on /dev/cu.usbmodem1101. Decoded with console_decode, side by side:
- reset 1: BOOT proto=1 fw=0.1.0 pipe=2048; one RIGCFG ... window_s=10 icache=0 dcache=0; STATUS sent=16, 21, 27 ... 7833 (66 records, sent advancing every ~1 s then during dump), dropped_full=0 bytes_dropped=0 trunc=0 ep_err=0 throughout; CAPSTAT delivered=30 expected=0 overrun=0 audio_exit=0 dumped=30; DUMPEND blocks=30 chunks=7650 bytes=983040 elapsed_ms=2376 dropped_full=0.
- reset 2: BOOT; one RIGCFG icache=0 dcache=0; STATUS sent=17 ... 7839 (67 records), dropped_full=0; delivered=30, audio_exit=0; DUMPEND blocks=30 chunks=7650 bytes=983040 elapsed_ms=2386 dropped_full=0.
(The reset-1 capture also begins with the boot the flash reset started, cut at about 7 s by my reset; it carries its own BOOT and RIGCFG icache=0 dcache=0 as a third boot.) dump_reassemble: reset 1 and 2 both complete=30 failed=0 abandoned=0, bad_frames=0 resyncs=0 discarded=0, 983040 PCM bytes (491520 samples, 10.24 s) each.

AC3: returned Left lane, first 0.5 s skipped. Reset 1: peak 3125 (-20.41 dBFS), RMS -23.43 dBFS, 1 kHz least-squares amplitude -20.42 dBFS, residual after removing the tone 0.33 LSB RMS, 120 distinct sample values. Reset 2: peak 3125 (-20.41), RMS -23.42, fit -20.41, residual 0.31 LSB. Expected -20.42 peak / -23.43 RMS (docs/reference/daisy-pod.md, -0.41 dB loop at -20 dBFS). Tolerance +/-0.5 dB: both within 0.02 dB. A stale, flat or repeating buffer cannot give a 1 kHz tone at the stimulus level with a 0.3 LSB residual, so the relocated SAI DMA buffers are moving samples.

Caveats: cpu_hz in RIGCFG reads 3.2 GHz and max_block_us/worst_gap_us read 0 because a probe flash/reset stops the DWT cycle counter (known, docs/reference/daisy-seed3.md); this ticket did not need timing. No memory was read through the probe during a captures. 'probe-rs info' (which ignores --chip) was run once after blinky; the board answered normally afterwards. AC5: nothing unexpected, no bug filed.

Captures archived: audio/captures/2026-10-13-rig-10s-reset1.console.zst, ...reset2.console.zst.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Flashed blinky (first bytes 00 00 08 24, vector table 24080000 / reset 08000299 inside the image) and rig (10 s window, FEATURES=seed3) from the post-TASK-059.01 tree over the probe path. Across two probe resets rig showed BOOT, one RIGCFG icache=0 dcache=0, STATUS sent advancing with dropped_full=0, all 30 blocks proved by dump_reassemble, and returned the 1 kHz tone at peak -20.41 dBFS / RMS -23.43 and -23.42 dBFS against the expected -20.42 / -23.43 (tolerance 0.5 dB, 0.3 LSB residual). So the SAI DMA buffers at the relocated addresses move samples. LED and listening remain TASK-059.03 (HUMAN). No bugs found.
<!-- SECTION:FINAL_SUMMARY:END -->
