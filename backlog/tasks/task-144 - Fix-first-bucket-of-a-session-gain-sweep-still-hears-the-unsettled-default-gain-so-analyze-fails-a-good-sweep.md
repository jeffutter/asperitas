---
id: TASK-144
title: >-
  Fix: first bucket of a session gain sweep still hears the unsettled default
  gain, so analyze fails a good sweep
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 20:17'
updated_date: '2026-10-19 20:17'
labels:
  - rig
  - host
  - measure
dependencies: []
references:
  - captures/TASK-144/2026-10-19-rig-sweep-gain-session-bootloaded.console.zst
  - captures/TASK-144/2026-10-19-rig-sweep-gain-session-bootloaded.report.json
  - captures/TASK-019.03.04/2026-10-15-rig-sweep-gain-sine.report.json
priority: medium
type: bug
ordinal: 270800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found on the bench (2026-10-19), TASK-099.05 third attempt, loopback cable fitted, bootloaded rig (a723f4c image, 60 s window). `asperitas-rig session --probe-reset --fx-f 1 --knob 1 ... sweep --from 0 --to 1 --duration 30 --start-after-arm 5` ran clean (600/600 SET acks, REL, dump_end). `asperitas-measure analyze` now loads it (after TASK-143, attach_prefix_bytes=121) but the verdict is FAIL: 'sweep response reverses by 55.2 dB (limit 1 dB)', span 9.6 dB, monotonic false.

The capture, its session sidecar and the report are committed under captures/TASK-144/.

Per-bucket comparison against captures/TASK-019.03.04/2026-10-15-rig-sweep-gain-sine.report.json: buckets 0.075 to 0.975 match the 2026-10-15 curve with a constant -2.1 dB offset (same 3.6 dB per bucket slope; the offset is level or cable loss on this setup). Only bucket 0.025 is wrong: -24.9 dB where the old run measured -81.5. That is about the level of the default gain before the first SET (-13 dB at gain 1 minus the bucket's share), so the bucket still contains audio from before the gain reached 0: either the gain smoother is still settling from its default when the sweep starts, or the stream-derived start (start_s 5.001 here, hand-set 4.9 in the old run) is early relative to the audible effect of the first SET.

Decide the fix and pin it: for example have the session send the 'from' level once before the capture arms (as hold does) so the sweep starts settled; or make analyze discount a settle interval after the first SET. Check which, with the device t_ms of CTLR OK SET and the block times in the committed capture, then confirm on the board.

Blocks TASK-099.05 (HUMAN): its sweep cannot pass analyze until bucket 0 is clean.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cause of the bucket-0 level identified from the stream (gain settling versus start estimate) and recorded in the ticket notes.
- [ ] #2 A gain sweep session of 0 to 1 over 30 s analyses as monotonic with no reversal, after the fix, in a host test that models the unsettled start.
- [ ] #3 gates.sh push passes.
<!-- AC:END -->
