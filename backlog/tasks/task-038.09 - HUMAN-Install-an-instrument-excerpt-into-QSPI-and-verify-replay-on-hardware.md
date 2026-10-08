---
id: TASK-038.09
title: 'HUMAN: Install an instrument excerpt into QSPI and verify replay on hardware'
status: Blocked
assignee:
  - '@human'
created_date: '2026-10-08 14:27'
updated_date: '2026-10-08 15:55'
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
- [ ] #1 HUMAN: one named excerpt from audio/instruments/ is installed into QSPI with its erase-inclusive wall-clock time recorded, the device reports EXCOK with a matching readback CRC, replay drives the DAC without gaps, and a person confirms by ear that the returned audio is recognisably that clip. The digital claim is exactness of the buffer; the analog judgement stays human.
- [ ] #2 HUMAN: the device-reported CRC of the samples handed to the DAC over one full pass equals the host-computed CRC of the excerpt, both values quoted
<!-- AC:END -->
