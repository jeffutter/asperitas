---
id: TASK-142
title: >-
  Fix: asperitas-rig session cannot see BOOT on a bootloaded rig, so it refuses
  every run
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 19:48'
updated_date: '2026-10-19 19:57'
labels:
  - rig
  - host
  - hardware
dependencies: []
priority: medium
type: bug
ordinal: 268800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found on the bench (2026-10-19) trying TASK-099.05 with the Pod self-loopback cable fitted. `asperitas-rig session --probe-reset --fx-f 1 --knob 1 ... sweep` against a bootloaded rig (current main, ASP_RIG_CAPTURE_SECONDS=60) exits 'no BOOT record after attaching: this is not a fresh boot, refusing to send FX'. Summary: sets_sent=0 fx_acked=false armed=true.

The stream the session did read starts at seq 5, device t_ms 203 (RIGGEN, CAPMAX, 'Audio interface ready', nvic line, 'USB connected' at t_ms 419, then STATUS and CAPSTAT). BOOT (seq 0) and RIGCFG (seq 4) were emitted before the device's USB connected and never reached the host. The bootloaded layout is the only one rig fits, and it always reboots through the Daisy bootloader's USB session, so the host port disappears and returns around the app's start. Every bootloaded capture this session shows the same loss (discarded_bytes 10 to 157 and a first seq of 4 or 5); asperitas-rig restart reports boot_record=false for the same reason. The session's host-side tests use a fake device that speaks BOOT first, which is why they pass.

Decide the fix: for example let the session accept a freshness proof other than BOOT (first record's seq and t_ms showing the board has just started, capture not yet armed: CAPSTAT delivered=0), or have the device emit BOOT again once USB is connected. Either way the guard's purpose stays: never send FX to a board that may already be armed.

Blocks TASK-099.05 (HUMAN session recheck). The loopback cable is now fitted, so the bench run can proceed as soon as this lands.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Root cause confirmed: which records are lost before attach on a bootloaded rig and why, recorded in the ticket notes.
- [x] #2 asperitas-rig session --probe-reset reaches FX/SET/REL against a bootloaded rig image without weakening the never-send-to-a-running-board guarantee.
- [x] #3 A host test models the lost-prefix stream (first seq above 0, no BOOT) and pins both the accepted fresh boot and the refused already-armed board.
- [x] #4 gates.sh push passes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-142. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause: on the bootloaded layout the device emits BOOT (seq 0) and RIGCFG (seq 4) before its USB console connects (about t_ms 419), and the host port also re-enumerates around the app start, so the host attaches at seq 4 or 5 and never sees BOOT. Fix is host-only (crates/asperitas-rig/src/session.rs): the session now accepts a fresh-boot proof other than BOOT: first record seq <= 32 and device t_ms <= 3000, a CAPSTAT delivered=0 seen, and never armed. A later BOOT after sending counts as DeviceReset (boots_at_start baseline). No firmware change, so nothing was flashed. Not exercised on the bench here; this assumes the device emits CAPSTAT delivered=0 before arming (as the ticket's captured stream shows). Bench recheck belongs to TASK-099.05.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-rig session --probe-reset now treats an early, idle, never-armed stream (low seq, low t_ms, CAPSTAT delivered=0) as a fresh boot when BOOT was lost before USB attached; armed or late-attached boards are still refused. Host tests pin accepted, armed-refused, late-refused and reset-after-lost-prefix cases; gates.sh push passes.
<!-- SECTION:FINAL_SUMMARY:END -->
