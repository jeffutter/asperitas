---
id: TASK-116.01
title: >-
  Docs: move console, rig, excerpt and host-control grammar from daisy-seed3.md
  into rustdoc with one protocol index
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 21:12'
updated_date: '2026-10-18 21:19'
labels:
  - docs
  - task
  - planned
dependencies: []
parent_task_id: TASK-116
priority: medium
ordinal: 256800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Move the console protocol v1, BOOT/STATUS, rig record grammar, USB short-packet rule, host control wiring, excerpt layout/format/record grammar and capture budgets out of docs/reference/daisy-seed3.md into rustdoc on frame.rs, console.rs, excerpt.rs and control.rs (the normative sources), reusing what is already there. Add one short protocol index doc page pointing at those modules. Do not leave transcriptions behind; the seed3 page keeps only a link. Comments carry no ticket numbers or history.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Each grammar section has a single rustdoc owner and the content is not lost
- [x] #2 A short protocol index exists and links the rustdoc owners
- [x] #3 cargo doc builds without new warnings
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 1f2f43d. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved into rustdoc: frame.rs (fields, examples, delimiter, short-packet rule), console.rs (BOOT/STATUS, rig record table), excerpt.rs (device-to-host records, storage per second, rules, replay CRC), capture.rs (derived budgets), host_control.rs (firmware wiring). Added docs/reference/protocol.md as the index; daisy-seed3.md keeps links plus measured SDRAM/QSPI rows. Repointed asperitas-rig.md, panictest.rs, CLAUDE.md; excerpt_docs.rs now checks the excerpt.rs module docs. Dropped the 8.8 percent truncated-line history (belongs to the earlier ticket notes). cargo doc -Dwarnings clean; asperitas-wire tests pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Console, rig, excerpt, capture and host-control grammar now live in rustdoc with a single owner each; protocol.md indexes them and the Seed3 page links instead of transcribing.
<!-- SECTION:FINAL_SUMMARY:END -->
