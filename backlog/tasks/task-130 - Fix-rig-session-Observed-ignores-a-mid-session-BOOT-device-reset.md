---
id: TASK-130
title: 'Fix: rig session Observed ignores a mid-session BOOT (device reset)'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 03:09'
updated_date: '2026-10-19 13:56'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 253800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-rig/src/session.rs Observed latches boot/armed/fx_acked/arm_t_ms for the whole attachment. A device reset after arming leaves stale state; the scheduler keeps sending SETs to a restarted device and outcome() can report Pass with a sidecar describing a session that did not happen. A BOOT after the session began should abort (or restart) the session with a distinct failure.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Second BOOT after the session sent a command aborts with a distinct DeviceReset error and outcome Error
- [x] #2 Unit test covers a mid-session BOOT; cargo test and clippy pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 293f6140594e51f45c2baa5f03b820479b86b61f. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Observed.boot became a boots counter; Runner::pump returns ControlError::DeviceReset (outcome Error) when boots > 1 after anything was sent. Test mid_session_boot_aborts_with_device_reset added; docs updated. cargo test, clippy, gates commit pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A second BOOT after the session sent a command now aborts with a distinct DeviceReset error and outcome Error.
<!-- SECTION:FINAL_SUMMARY:END -->
