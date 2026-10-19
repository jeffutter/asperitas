---
id: TASK-144
title: >-
  Fix: first bucket of a session gain sweep still hears the unsettled default
  gain, so analyze fails a good sweep
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 20:17'
updated_date: '2026-10-19 21:24'
labels:
  - rig
  - host
  - measure
  - planned
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
- [x] #1 Cause of the bucket-0 level identified from the stream (gain settling versus start estimate) and recorded in the ticket notes.
- [x] #2 A gain sweep session of 0 to 1 over 30 s analyses as monotonic with no reversal, after the fix, in a host test that models the unsettled start.
- [x] #3 gates.sh push passes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-144. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cause (AC1), from the committed capture: re-running analyze with the sidecar first_set_t_ms shifted shows bucket 0.025 stays near -24 dB until the sweep start is moved about 0.7 s later (-37 dB at +0.5 s, clean -82 dB at +0.7 s; trimmed window starts 0.375 s into the bucket). So audio at the unsettled default gain (-13 dB at gain 1) reaches roughly 0.9 s past the device t_ms of the first SET (8206 ms, arm 3205 ms), far longer than smoother settling; the same lag explains the constant -2.1 dB offset of every other bucket (3.6 dB per 1.5 s bucket = 2.4 dB/s x 0.9 s). Either way the first SET is the first thing to move the gain, so the fix is to leave nothing unsettled: Motion::Sweep now sends the from level once before the capture arms (Timeline.pre, as Hold does). analyze is unchanged. Tests: session.rs timeline/order/never-arms tests updated; sweep.rs unsettled_start_fails_and_settled_start_passes models the 300 ms unsettled start. docs/reference/asperitas-rig.md updated. Bench confirmation remains TASK-099.05.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Session gain sweeps now send the 'from' level before the capture arms so bucket 0 is settled; host tests pin it; gates.sh push passes (19 gates). Bench confirmation is TASK-099.05.
<!-- SECTION:FINAL_SUMMARY:END -->
