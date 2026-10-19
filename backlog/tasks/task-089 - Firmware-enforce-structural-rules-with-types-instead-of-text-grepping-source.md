---
id: TASK-089
title: 'Firmware: enforce structural rules with types instead of text-grepping source'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 17:27'
labels:
  - review-2026-10
  - firmware
  - planned
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
- [x] #1 build.rs rig_flash_ban and the executor grep script are deleted
- [x] #2 Flash writes cannot compile in the blocking form (type-enforced)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 2ca2119. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added firmware/src/bin/rig/flash.rs: AsyncFlash newtype with private field, async-only methods, sole build_async call. Deleted rig_flash_ban from build.rs. The executor grep script was already gone (audio::start carries it). Negative check: a blocking flash.read call fails with E0599. gates.sh commit passes. Conditional dead_code allowances cover feature variants that do not use flash.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rig flash is now type-enforced via AsyncFlash; the build.rs text ban is removed.
<!-- SECTION:FINAL_SUMMARY:END -->
