---
id: TASK-089
title: 'Firmware: enforce structural rules with types instead of text-grepping source'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - firmware
dependencies:
  - TASK-085
priority: low
type: chore
ordinal: 191800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
build.rs:201-236 rig_flash_ban bans .read(, .write( and .build() anywhere in rig.rs code (its own comment at :206 says 'A type cannot enforce that'); it also blocks any unrelated .read( added to rig. scripts/check-main-audio-executor.sh greps main.rs for executor patterns.

Change: wrap the flash in struct AsyncFlash(Flash<Async>) with a private field and only async methods in its own module, whose constructor is the only build_async call; rely on the shared audio::start for the executor guarantee; delete both text checks.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 build.rs rig_flash_ban and the executor grep script are deleted
- [ ] #2 Flash writes cannot compile in the blocking form (type-enforced)
<!-- AC:END -->
