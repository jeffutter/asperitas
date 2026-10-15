---
id: TASK-038.09
title: 'HUMAN: Install an instrument excerpt into QSPI and verify replay on hardware'
status: Done
assignee:
  - '@human'
created_date: '2026-10-08 14:27'
updated_date: '2026-10-15 18:39'
labels:
  - planned
dependencies:
  - TASK-038.04
parent_task_id: TASK-038
priority: medium
ordinal: 130800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Split out of TASK-038.05 AC #5 on 2026-10-08 at the owner's request. Everything else in TASK-038.05 was verified on the bench, but excerpt replay depends on TASK-038.04, which was not built, so this check waits for it instead of holding the rig's verification ticket open.

Bench notes from TASK-038.05 that apply here: flash with the probe, then boot with RESET (a probe flash stops the DWT). Hold the console with a single long-lived reader (a restarted reader loses buffered data). The loop is straight and -0.41 dB at -20 dBFS (docs/reference/daisy-pod.md), so a replayed excerpt's returned level can be predicted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: one named excerpt from audio/instruments/ is installed into QSPI with its erase-inclusive wall-clock time recorded, the device reports EXCOK with a matching readback CRC, replay drives the DAC without gaps, and a person confirms by ear that the returned audio is recognisably that clip. The digital claim is exactness of the buffer; the analog judgement stays human.
- [x] #2 HUMAN: the device-reported CRC of the samples handed to the DAC over one full pass equals the host-computed CRC of the excerpt, both values quoted
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15: closed in a live session with the owner at the bench; the owner agreed both criteria were met. Two bootloaded rig images, each flashed with a RESET tap and dfu-util -w to 0x90040000 (no probe): (1) default rig, ASP_RIG_CAPTURE_SECONDS=10, used for the install because a stim-excerpt build refuses installs until its pass completes; (2) stim-excerpt, ASP_RIG_EXCERPT_SLOT=0, same window. Clip: audio/instruments/mandolin_single_note_soft.wav (mono 48 kHz 16-bit, 3.00 s, 288,000 B). Install: excerpt_stream to slot 0 name mand_soft, 492,867 bytes written after DUMPEND, EXCOK name=mand_soft bytes=288000 crc16=15fd, 5.82 s from first byte to verdict (includes USB, erase, program, readback; the doc predicted at most 22.5 s). Replay: EXCPLAY slot=0 bytes=288000 crc16=15fd want=15fd underruns=0 clean=1; gates delivered 30 == expected 30, overrun 0, max_block_us 15, worst_gap_us 674; dropped_full 0; dump_reassemble 0 bad frames. Host CRC-16/CCITT-FALSE of the WAV data chunk: 15fd. AC #2: device 15fd == host 15fd. AC #1 analog: the returned capture aligns at 81 samples (1.69 ms), gain 0.953 (-0.42 dB, the loop gain), correlation 0.99959, and the owner heard the original and the returned clip as identical. Recorded in the daisy-seed3.md cost table (observed cells for install wall clock, wire bytes, USB time, replay margin) and README (63ff25c); captures archived as audio/captures/2026-10-15-rig-excerpt-{install,replay}-mand-soft.console.zst. Not measured: the replay staging margin itself and USB time separately from the install. The CRC proves only what reached the DAC encoder, not the codec or cable.
<!-- SECTION:NOTES:END -->
