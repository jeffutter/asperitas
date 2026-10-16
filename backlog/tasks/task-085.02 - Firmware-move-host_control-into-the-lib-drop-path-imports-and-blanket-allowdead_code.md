---
id: TASK-085.02
title: >-
  Firmware: move host_control into the lib, drop #[path] imports and blanket
  allow(dead_code)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:00'
updated_date: '2026-10-16 20:17'
labels:
  - planned
dependencies:
  - TASK-085.01
parent_task_id: TASK-085
priority: high
ordinal: 225800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
host_control.rs is shared by main.rs and podtest.rs through #[path] with a blanket allow(dead_code). Make it a lib module (depends on the lib target ticket), import it normally in main and podtest, and remove the #[path] attributes and the allow. Fix any genuinely dead items instead of allowing them. Acceptance: no #[path] in firmware/src/bin; no blanket allow(dead_code); all builds and gates pass.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 771d544. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
host_control is now 'pub mod' in the firmware lib; main and podtest import it via asperitas_firmware::host_control. Removed #[path] and blanket allow(dead_code). version() and its imports are now cfg(log-usb), the only genuinely dead items in the RTT-only build. Docs updated. gates.sh push passes. No acceptance criteria were defined on the ticket.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved host_control into the lib, removed #[path] imports and the blanket dead_code allow, gated console-only items on log-usb, updated daisy-seed3.md.
<!-- SECTION:FINAL_SUMMARY:END -->
