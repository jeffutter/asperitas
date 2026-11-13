---
id: TASK-152.04
title: asperitas-rig session plays the same automation file on the device
status: To Do
assignee:
  - '@agent'
created_date: '2026-11-13T22:01:00Z'
labels:
  - automation
  - rig
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
- [ ] #1 asperitas-rig session accepts an automation file as a motion and sends each knob's changes at the times the file implies, over the existing control channel with no firmware change; hold, sweep and soak behave as before
- [ ] #2 The session sidecar records the automation (inline or by checksummed reference) so a capture is self-describing, and a capture taken with a different file than the one named is detected
- [ ] #3 asperitas-measure analyze --excerpt runs the reference for an automated session at that schedule through the same offline render as TASK-152.01, and refuses a session it cannot reproduce
- [ ] #4 Host tests with a recorded or synthetic control exchange cover the schedule sent for hold, ramp and step forms, and a malformed or out-of-range file is refused before anything is sent to the device
- [ ] #5 docs/reference/asperitas-rig.md documents the new motion with a command example
- [ ] #6 bash scripts/gates.sh push passes
<!-- AC:END -->
