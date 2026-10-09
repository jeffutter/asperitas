---
id: TASK-038.08
title: >-
  Start rig's one-shot stimuli inside the capture window so the archive holds
  the whole sweep
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 14:03'
updated_date: '2026-10-09 02:19'
labels:
  - planned
dependencies: []
parent_task_id: TASK-038
priority: medium
ordinal: 129800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found on the bench 2026-10-08 (TASK-038.05 notes). With `stim-ess`, the 8 s exponential sweep (384 000 samples, 20 Hz-20 kHz) plays once, starting at the first audio callback. rig only arms the capture `ARM_DELAY_MS` (2 s) later, so the recording starts at about 150 Hz: the first 2 s of the sweep (20-112 Hz) are never captured, and the remaining 24 s of a 30 s window are silence. A deconvolution or frequency-response analysis (TASK-035) needs the whole sweep in the capture, with a known start sample.

The sine and pulse train are periodic and unaffected. Something has to tie the one-shot sweep's start to the capture: (re)start the generator at the armed callback, or arm first and start playback on a block boundary inside the window. Either way, record the stimulus start as a sample offset in the capture (a field on an existing record, or a new one), so the host need not find it by cross-correlation. Keep the arm delay's purpose: boot descriptors drain before any capture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With stim-ess, the archived capture contains the sweep from its first sample (20 Hz) to its last, and the stimulus start offset within the capture is reported by the device in a record a host tool can read
- [x] #2 Sine and pulse-train builds are unchanged in behaviour, and the default build's gates and timeline are unchanged
- [x] #3 A host test or const assert pins the new start-of-stimulus bookkeeping, and `scripts/gates.sh push` passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit with trailer Task-Id: TASK-038.08 and subject 'TASK-038.08: start rig's one-shot sweep at the first captured callback and report it in STIMSTART' (a sha cannot be cited here: amending the ticket into that commit changes it). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation (nothing of this existed at HEAD ee30f1a):
- asperitas-dsp: Stimulus::one_shot_samples() -> Option<u32>, default None (periodic); ExponentialSweep returns Some(total_samples). rig keys the start rule off this rather than off cfg(stim-ess), so a future one-shot generator gets the same treatment.
- rig.rs: the audio callback now captures before it renders (entry/finish timing still brackets everything; output is not played until the DMA reaches it next period, so the order changes no sample). A one-shot generator is held silent (frames_out stays zero) until the first callback whose input lands in the ring, then reset() and started in that same callback, which publishes the block seq and callback index via STIM_BLOCK/STIM_CALLBACK/STIM_STARTED (release). The arm delay is untouched, so BOOT/RIGCFG/RIGGEN/CAPMAX still drain before any capture.
- After the window closes (after judge_capture, before the dump), a one-shot build logs 'rig: gate stimulus samples a..b inside the N-sample capture: pass|FAIL' and emits the new console verb 'STIMSTART proto=1 first_block=<seq> offset=<samples>'. offset is the sample index in the PCM dump_reassemble writes; it is 0 by construction, and the echo arrives one loop latency later (the record says where playback began). console_decode prints it like any other record.
- asperitas-logging: console::StimulusStart + stimstart_body (field table, const worst-case assert 59 B, pinned-text test, saturated test, codec round trip); capture::window_sample_offset(first_block, block, callback) -> Option<u32>, refusing positions before the window, past a block, or past the ring.
- Host tests: stimulus_tests only_the_sweep_is_one_shot_and_it_reports_its_exact_length; capture_geometry window_offset_is_the_sample_index_in_the_reassembled_pcm and window_offset_refuses_positions_outside_the_window; console stimstart_body_pins_field_names_and_order plus the shared saturated/codec tests.
- AC #2: sine and pulse builds have one_shot_samples() == None, so playing starts true and process_block runs every callback as before; run_capture skips report_stimulus_start; no STIMSTART is sent. Default build timeline and gates are unchanged.
- Flash: stim-ess rig image .text 127146 + .data 408 = 127554 B of 131072 (3.5 KB left); default 125746, pulse 125970.
- scripts/gates.sh push: 28 gates pass. Not run on the bench: AC #1's 'archived capture contains the sweep' is met by construction and reported by the on-device gate line, but no stim-ess capture has been taken with this build yet. The README rig section says so.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig now starts a one-shot stimulus (the 8 s exponential sweep) at the first captured callback instead of at boot, so the archived capture holds the whole sweep from 20 Hz. Generators declare one-shot-ness via the new Stimulus::one_shot_samples(); sine and pulse train return None and behave as before. After the window rig checks the sweep fit inside the capture (gate line) and emits a new STIMSTART record (first_block, offset in samples) so a host need not cross-correlate to find the start. Host tests pin the offset arithmetic (capture::window_sample_offset), the STIMSTART wire text and the one-shot contract; scripts/gates.sh push passes. Not yet run on the bench.
<!-- SECTION:FINAL_SUMMARY:END -->
