---
id: TASK-085.02
title: >-
  Firmware: move host_control into the lib, drop #[path] imports and blanket
  allow(dead_code)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 20:00'
updated_date: '2026-10-16 20:00'
labels:
  - task
  - firmware
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
