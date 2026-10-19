---
id: TASK-099
title: >-
  Rig: one session command that writes the raw stream plus a sidecar; retire
  scripts/rig-session.py
status: Done
assignee:
  - '@human'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 21:35'
labels:
  - review-2026-10
  - rig
  - host
  - planned
dependencies:
  - TASK-095
  - TASK-099.01
  - TASK-099.02
  - TASK-099.03
  - TASK-099.04
  - TASK-099.05
priority: medium
type: chore
ordinal: 201800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
asperitas-rig capture/override writes a records-only text artifact but asperitas-measure reads only a raw .console.zst, so no rig output can be analysed; the evidence behind TASK-019.03.04 came from scripts/rig-session.py, which re-implements the 20/s SET schedule and level-to-wire mapping (its wire() clamps where rig's level_to_wire refuses out-of-range, control.rs:322), has no REL-on-error guarantee, shells out to the ctl_frame example per frame, and hardcodes /dev/cu.usbmodem101. Measure docs claim captures come from asperitas-rig override --sweep, which is false. --sweep-start-s is a hand estimate (thresholds.rs:180-186 trims buckets to tolerate it) because nothing records when the first SET landed.

Change: one rig session path (optional probe-reset, FX on attach, wait for arm, SET schedule, always-REL, stop at DUMPEND) writing the raw zstd stream plus a sidecar (fx, schedule, device t_ms of first SET ack); measure analyze reads the sidecar instead of --fx/--sweep/--sweep-start-s; delete the script.
DECISIONS NEEDED: should the session also own probe-rs reset or only the console side; take motion timing from the stream itself (t_ms on CTLR OK SET) or a host-written sidecar.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A rig session produces input that asperitas-measure analyses with no hand-set flags
- [x] #2 scripts/rig-session.py is deleted
- [x] #3 A bench run reproduces a TASK-019.03.04 style result (hardware recheck ticket)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against b1f171b
Approach: one 'asperitas-rig session' path replaces scripts/rig-session.py; timing comes from the stream (device t_ms of first CTLR OK SET) with a sidecar fallback; measure analyze reads the sidecar.
Order: TASK-099.01 (stream timing), TASK-099.02 (session subcommand), TASK-099.03 (measure reads sidecar), TASK-099.04 (delete script, docs), TASK-019.03.04 (HUMAN bench recheck).
Parent is @human because TASK-019.03.04 is. AC1 satisfied by TASK-099.03, AC2 by TASK-099.04, AC3 by TASK-019.03.04. Final test: cargo test -p asperitas-rig -p asperitas-measure -p asperitas-host, then bench run.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Owner delegates the open questions: the session command owns the console side and takes an optional probe-reset flag; motion timing comes from the stream itself (device t_ms on the first CTLR OK SET) if that is enough, otherwise a sidecar.

2026-10-19, live session with the owner: all subtasks are done and the owner signed off the bench run (TASK-099.05). AC #1 by TASK-099.03, AC #2 by TASK-099.04, AC #3 by TASK-099.05.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
One asperitas-rig session command writes the raw stream plus a sidecar, asperitas-measure analyze reads it with no hand-set flags, and scripts/rig-session.py is gone. The bench run reproduced the TASK-019.03.04 gain sweep on a bootloaded rig, after three bench-found fixes (TASK-142, TASK-143, TASK-144).
<!-- SECTION:FINAL_SUMMARY:END -->
