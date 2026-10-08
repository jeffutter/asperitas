---
id: TASK-038.08
title: >-
  Start rig's one-shot stimuli inside the capture window so the archive holds
  the whole sweep
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-10-08 14:03'
updated_date: '2026-10-08 15:55'
labels:
  - planned
  - ready-for-agent
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
- [ ] #1 With stim-ess, the archived capture contains the sweep from its first sample (20 Hz) to its last, and the stimulus start offset within the capture is reported by the device in a record a host tool can read
- [ ] #2 Sine and pulse-train builds are unchanged in behaviour, and the default build's gates and timeline are unchanged
- [ ] #3 A host test or const assert pins the new start-of-stimulus bookkeeping, and `scripts/gates.sh push` passes
<!-- AC:END -->
