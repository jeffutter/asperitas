---
id: TASK-152.04
title: asperitas-rig session plays the same automation file on the device
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-13T22:01:00Z'
updated_date: '2026-11-19T02:54:00Z'
labels:
  - automation
  - rig
  - planned
dependencies:
  - TASK-152.01
documentation:
  - docs/reference/asperitas-rig.md
  - docs/reference/protocol.md
parent_task_id: TASK-152
priority: medium
type: feature
ordinal: 291700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The rig `session` command drives a knob on the board through hold, sweep and soak schedules that are defined only in the rig tooling. With the automation file from TASK-152.01 there would be two definitions of knob motion, one for offline renders and one for the device, and a device-versus-offline comparison at moving settings would compare two schedules instead of one.

Make `asperitas-rig session` accept the automation file as a motion, sending `CTL SET` for each knob at the times the file implies, so the same file drives the offline render and the board. The device applies knob overrides per block; the wire carries a normalised level, so the session must send what the shared translation gives for each real-unit value. The existing hold, sweep and soak motions keep working. `asperitas-measure analyze --excerpt` judges a session by its sidecar; it must be able to run the reference for an automated session at the same schedule, so the sidecar must record the automation.

This is host-side only: firmware must not need to change. If it turns out it must, stop and record why in the notes before continuing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-rig session accepts an automation file as a motion and sends each knob's changes at the times the file implies, over the existing control channel with no firmware change; hold, sweep and soak behave as before
- [x] #2 The session sidecar records the automation (inline or by checksummed reference) so a capture is self-describing, and a capture taken with a different file than the one named is detected
- [x] #3 asperitas-measure analyze --excerpt runs the reference for an automated session at that schedule through the same offline render as TASK-152.01, and refuses a session it cannot reproduce
- [x] #4 Host tests with a recorded or synthetic control exchange cover the schedule sent for hold, ramp and step forms, and a malformed or out-of-range file is refused before anything is sent to the device
- [x] #5 docs/reference/asperitas-rig.md documents the new motion with a command example
- [x] #6 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d1648c2. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Parser moved to asperitas_dsp::automation (std feature; asperitas-cli re-exports it) because measure cannot depend on cli and rig depends on neither; firmware no_std build unaffected (gate passed). Automation::device_schedule is the one device schedule: steps one update at the latching block, ramps sampled every 480 frames (10 ms) rather than per block (1500 replies/s on the console); the CTL SET channel carries this, no firmware change. Session sends all knobs at their start values before arming (unnamed knobs at default so the Pod knobs are not followed) and releases all after the last update. Sidecar: new mode=automation, no knob line (Sidecar.knob and SessionPlan.knob are Option), CRC-32 (crc32fast, already in lockfile) plus escaped one-line script; parse refuses a checksum mismatch. The rig reads the file once so capture and sidecar cannot name different scripts; detection covers a sidecar edited afterwards. measure --excerpt builds the reference via Automation::for_each_block (same 32-frame grid as render) with script time zero at the excerpt start (= arm); host test plays the 10 ms staircase through wire quantisation and stays within tolerance, while a shifted or constant script fails. Known gap: device-versus-offline on hardware is TASK-152.05 (HUMAN).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-rig session now takes 'automation FILE' as a motion, sending CTL SET per Automation::device_schedule, with the script and its CRC-32 recorded in the sidecar. asperitas-measure analyze --excerpt reproduces the reference from the sidecar script through the shared block-grid render and refuses tampered, unparseable or non-excerpt use. Docs updated; bash scripts/gates.sh push passes.
<!-- SECTION:FINAL_SUMMARY:END -->
