---
id: TASK-027.02
title: >-
  Measure achieved poll rate and check for catch-up bursts from a scripted
  capture
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 01:29'
updated_date: '2026-10-15 05:55'
labels:
  - planned
dependencies:
  - TASK-027.01
  - TASK-030
  - TASK-031
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-027
priority: high
type: task
ordinal: 112
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-027's four criteria are measurements: take a capture, compute the median interval between knob records, check for catch-up bursts, write down the number. They carry a `HUMAN:` prefix because nobody could run the capture unattended, not because they need judgement.

With a runner that produces integrity-checked captures, the only human part is flashing and leaving the board cabled — which TASK-027.01 isolates. This ticket is what remains: the analysis, done by a program, reporting a number instead of an impression. That distinction matters here specifically — TASK-025 was once marked Done while its own hardware criterion sat unchecked, and a recorded measurement is harder to quietly skip than a checkbox.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A capture of at least 240 s is taken through the rig runner and its integrity summary shows no failed frames, so the interval measurement below rests on trustworthy records.
- [x] #2 The median inter-record interval for knob records is computed from timestamps and reported, with spread; roughly 10 ms is expected for KNOB_LOG_THROTTLE=10 at a true 1 kHz poll rate, not the ~16 ms of the pre-TASK-025 625 Hz build.
- [x] #3 No back-to-back burst of knob records with near-zero interval appears — the failure mode TASK-026 bounds. If one does appear, a bug ticket is filed carrying the timestamps from the artifact, and this criterion is left unchecked.
- [x] #4 Median and spread are recorded as numbers in TASK-027's implementation notes, and TASK-025 AC #3 is checked via task_edit only once the measured interval supports it.
- [x] #5 The capture artifact is committed or attached so the number can be re-checked rather than trusted.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this ticket's commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Probe 2026-10-15: /dev/cu.usbmodem1101 (rigged path in TASK-027.01) does not exist; /dev/cu.usbmodem1102 (5 s) gave records=0, verdict=fail no_status_record; /dev/cu.usbmodem101 produced no output. Bench is not in the rigged state (board missing or not running podtest). No measurement taken or fabricated. Next step: human re-flashes podtest and re-cables per TASK-027.01, confirms the device path, then rerun the plan.

2026-10-15: Bench is rigged again. The earlier probe failed because the board was running firmware that wedged the USB console (open() on the port hung). Re-flashed with 'make probe-flash BINARY=podtest' from firmware/ (verify OK, reset). Console now opens: the 'Asperitas Debug Console' is /dev/cu.usbmodem101 (the USB number changes on re-enumeration; the /dev/cu.usbmodem1101 path in TASK-027.01 is stale). /dev/cu.usbmodem1102 is the ST-Link VCP, NOT the console. Always pass --device /dev/cu.usbmodem101 to asperitas-rig (two usbmodem devices make auto-detect exit 2). Sanity capture: 5 s gave 544 records, bad_frames=0, accounting_ok=true, podtest lines at 10 ms cadence, but verdict=fail with device_dropped=786 and 52 discarded bytes, because the capture attached several seconds after reset and the device had already dropped records into its ring before a host was connected. Use --max-drops large enough to cover the pre-attach drops, or reset the board right before capturing (probe-rs reset --chip ASPERITAS_H750IB --chip-description-path firmware/asperitas-h750.yaml --non-interactive --connect-under-reset), and report the drop count and the first_gap boundary in the measurement; compute interval and burst statistics only over records after the gap. If open() hangs again, do not retry blindly: re-flash podtest via the probe path and record that.

2026-10-15 measurement (TASK-027.02): captures/task-027-02-podtest-knob.txt, 250 s via asperitas-rig (bad_frames=0, accounting_ok=true, 25360 records, one leading gap seq 0x25->0x1bb with device_dropped=405 and 49 discarded bytes from pre-attach, so rig verdict=fail on discarded_bytes only; stats use records after the gap). Knob interval from body t= over 24828 deltas (scripts/knob-interval.py): median 10.0 ms, mean 10.071, p5 10, p95 11, min 10, max 11, stdev 0.257 ms (1 ms quantisation). No deltas <=2 ms, no stalls >20 ms, so no catch-up bursts. Controls assumed untouched. Consistent with true 1 kHz poll rate. Artifact and scripts/knob-interval.py committed. TASK-025 AC #3 checked since median is 10.0 ms with no burst. Note: the AC1 wording (no failed frames) is met; the rig verdict itself is fail only because of pre-attach discarded bytes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Took a 250 s integrity-checked capture of podtest; knob interval median 10.0 ms, stdev 0.26 ms, no bursts or stalls, confirming ~1 kHz. Added scripts/knob-interval.py and committed the artifact; checked TASK-025 AC #3.
<!-- SECTION:FINAL_SUMMARY:END -->
