---
id: TASK-073.01
title: >-
  Repeat 240 s podtest capture on hardware to see whether the console drop
  window recurs
status: Done
assignee:
  - '@human'
created_date: '2026-10-15 12:10'
updated_date: '2026-10-15 14:11'
labels:
  - planned
  - task
dependencies: []
parent_task_id: TASK-073
priority: medium
ordinal: 160800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Human-run hardware step for TASK-073. Plug in the board and repeat the 240 s podtest capture several times, committing artifacts, to learn whether the seq_gap/dropped_full window seen in TASK-033.01 recurs and where.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: Run at least 3 captures with 'cargo run -p asperitas-rig -- capture --duration 240 --max-drops 100000 --out captures/task-073-run-N.txt' with nothing else reading the port; commit the artifacts
- [x] #2 HUMAN: Record per run: first_gap, device t_ms of the gap, dropped_full, bytes_dropped, and whether the host machine was busy/sleeping or the terminal was suspended
- [x] #3 HUMAN: Write the conclusion (recurs / does not recur, host vs device suspected) into TASK-073's notes and agree it with the agent in a live session
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 7a2ad00. Hardware only. Run the rig captures, compare seq_gap windows across runs (same device t_ms suggests device-side, e.g. a periodic USB/flash stall; random suggests host drain). Device counters dropped_full/pipe_free in the STATUS record bracket the window. Compare to captures/task-033-01-podtest-240s.txt.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Done in a live session with the owner (hardware access over ssh; Mac display was off for every run). Console is /dev/cu.usbmodem101; /dev/cu.usbmodem1102 is the ST-Link VCP and yields nothing, so --device is required. Artifacts committed in 70921f2.

| Run | Start | records | seq_gaps | first_gap | dropped_full start->end | bytes_dropped |
|---|---|---|---|---|---|---|
| 1 | long-running device (~57 min uptime, unread), not restarted | 24342 | 1 | 00053bcc->00053f29 (device t 3419861->3428441 ms, 8.6 s, 43 records into capture) | 343427 -> 343427 | 22186527 |
| 2 | asperitas-rig restart first | 24342 | 1 | 0000006d->0000006f (1 record, ~0.7 s in) | 31 -> 31 | 1872 |
| 3 | asperitas-rig restart first | 24336 | 0 | none | 30 -> 30 | 1810 |

Host busy/sleep: display off, ssh session, nothing else on the port.

Original criterion #3 (repeat with display awake) was waived by the owner: runs showed no mid-run stall with the display off, so there was nothing to separate.

Findings: dropped_full is flat for the whole 240 s in every run; all drops and the only gaps happen before or at host attach. After a fresh boot the device drops ~30 records before the host opens the port. The gap size scales with how long the device ran unread before attach (1 record fresh, 8.6 s after ~57 min; TASK-033.01's 16 s window fits). Rig verdict "fail" on runs 2/3 came from discarded_bytes (2 and 11, partial frame at stream start), not drops.
<!-- SECTION:NOTES:END -->
