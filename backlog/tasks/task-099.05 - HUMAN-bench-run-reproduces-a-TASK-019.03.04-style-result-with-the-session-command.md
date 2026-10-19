---
id: TASK-099.05
title: >-
  HUMAN: bench run reproduces a TASK-019.03.04 style result with the session
  command
status: Done
assignee:
  - '@human'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-19 21:34'
labels:
  - task
dependencies:
  - TASK-099.04
  - TASK-142
  - TASK-143
  - TASK-144
parent_task_id: TASK-099
priority: medium
ordinal: 249800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
HUMAN: with the board and probe attached, run asperitas-rig session then asperitas-measure analyze, and confirm the result matches the TASK-019.03.04 evidence. Commit artifacts with this ticket per docs/reference/asperitas-rig.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: asperitas-rig session runs a gain sweep on a bootloaded rig with the loopback cable and gets every SET and the REL acknowledged.
- [x] #2 HUMAN: asperitas-measure analyze passes the capture with no hand-set flags.
- [x] #3 HUMAN: the sweep curve matches the TASK-019.03.04 gain-sweep evidence in shape, with a constant level offset from the cable.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-19 bench attempt with the owner (loopback cable fitted, rig built from a723f4c with ASP_RIG_CAPTURE_SECONDS=60): asperitas-rig session --probe-reset refused to send FX because the BOOT record never reaches the host on a bootloaded rig (stream starts at seq 5). Nothing was swept and there is nothing to analyse. Filed as TASK-142; rerun this ticket once it lands.

2026-10-19 second bench attempt with the owner, after TASK-142: the session ran clean against a bootloaded rig (60 s window, a723f4c image, loopback cable fitted): gain sweep 0 to 1 over 30 s, fx_acked=true, sets_sent=600, set_acks=600, rel_acks=1, device_errors=0, dump_end=true, 176 dump blocks, first_set_t_ms=8206. analyze refused the capture because of 121 discarded bytes of lost prefix before the first valid frame. Filed as TASK-143. The raw stream is kept in the session scratchpad (sweep-gain2.console.zst plus .session sidecar) and should be rerun for the committed artifact once TASK-143 lands, since the board image does not change.

2026-10-19 third attempt, after TASK-143: analyze now loads the real bootloaded session capture (attach_prefix_bytes=121). Result: 19 of 20 gain buckets match the 2026-10-15 curve at a constant -2.1 dB offset with the same slope; bucket 0.025 reads -24.9 dB against -81.5, so the verdict is FAIL (reversal 55.2 dB). Filed as TASK-144 with the capture committed under captures/TASK-144/. This ticket's sweep reruns once it lands.

2026-10-19, live session with the owner; the owner agreed all three criteria are met and signed off the close. Fourth attempt, after TASK-142, TASK-143 and TASK-144: bootloaded rig built from a723f4c with ASP_RIG_CAPTURE_SECONDS=60, loopback cable fitted. asperitas-rig session --probe-reset --fx-f 1 --knob 1 sweep --from 0 --to 1 --duration 30 --start-after-arm 5: fx_acked=true, sets_sent=601, set_acks=601, rel_acks=1, device_errors=0, dump_end=true. asperitas-measure analyze (no flags): pass, rising, monotonic, max_reversal_db 0.0, span 67.7 dB, sweep start 5.0 s derived from the stream, attach_prefix_bytes 121. Against captures/TASK-019.03.04/2026-10-15-rig-sweep-gain-sine.report.json (span 68.2 dB) all 20 buckets differ by a constant -2.1 dB (bucket 0 by -1.5 dB), the same slope; the offset is this cable setup's level. Artifacts: captures/TASK-099.05/2026-10-19-rig-session-sweep-gain-bootloaded.{console.zst,console.zst.session,report.json}. Getting here took three fixes found at the bench: TASK-142 (BOOT lost on a bootloaded rig), TASK-143 (analyze rejected the attach prefix), TASK-144 (sweep bucket 0 unsettled).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The session command was verified on hardware against the TASK-019.03.04 gain sweep. Every SET and the REL were acknowledged, analyze passed with no hand-set flags, and the curve matches the 2026-10-15 evidence in shape with a constant -2.1 dB cable offset. The owner signed off in a live session.
<!-- SECTION:FINAL_SUMMARY:END -->
