---
id: TASK-005.02
title: Verify audio in and out on real hardware
status: Done
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-05 14:11'
labels: []
dependencies:
  - TASK-005.01
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/daisy-pod.md
parent_task_id: TASK-005
type: feature
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board, the Pod, and ears. Reproduce the checks PR #80's author ran on this same Seed3-in-Pod configuration.

The Pod's audio I/O is **line level, not hi-Z instrument level** — feed it from a DI, preamp, or an interface's line out. Do not plug a mandolin pickup straight in and conclude the codec is broken. See docs/reference/daisy-pod.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: audio into the Pod is audible at its output
- [x] #2 HUMAN: left and right channels verified not swapped
- [x] #3 HUMAN: line-out to line-in loopback passes audio repeatably across resets
- [x] #4 HUMAN: board boots and resets reliably; USB connects and reconnects
- [x] #5 Any deviation from the SAI config in docs/reference/daisy-seed3.md is corrected there
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified on hardware 2026-08-05. Passthrough works: audio into the Pod is audible at its output, left and right are not swapped, line-out to line-in loopback passes audio repeatably across resets, and the board boots/resets reliably with USB connecting and reconnecting. All four HUMAN criteria confirmed by the owner, not inferred from a build.

AC #5 — no deviation to correct. The firmware does not configure SAI itself: main.rs calls `prepare_interface(Default::default())`, so the whole SAI setup is daisy-embassy's. Every item in docs/reference/daisy-seed3.md's "SAI configuration" section was re-checked against src/codec/tac5242.rs at the pinned commit ca9bcc9 and matches exactly — SAI1 split A=TX master / B=RX slave with SyncInput::Internal, frame_length 64, DataSize::Data32, BitOrder::MsbFirst, FrameSyncPolarity::ActiveHigh, FrameSyncOffset::OnFirstBit, frame_sync_active_level_length U7(32), ClockStrobe::Falling on TX and Rising on RX, FifoThreshold::Quarter, master_clock_divider derived from fs, u32 sample words, and the 2 ms pre-clock startup delay. The doc needed no edit.

Also confirmed AudioConfig::default() is Fs::Fs48000, so the passthrough that was verified really is running at the 48 kHz this ticket targets rather than an unexamined default.

One place the parent ticket's text still differs from reality: TASK-005 targets a 48-sample block, but daisy-embassy hardcodes BLOCK_LENGTH = 32 (audio.rs:13). That is known and already documented under TASK-011 — it is not a new finding and did not affect verification.
<!-- SECTION:NOTES:END -->
