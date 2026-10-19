---
id: TASK-104
title: Remove dead public surface in pod and measure
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 02:05'
labels:
  - review-2026-10
  - pod
  - measure
  - planned
dependencies: []
priority: low
type: chore
ordinal: 206800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Unused or test-only public items: asperitas-pod lib.rs:47-48 (no-op init()), knob_override.rs:67,72 (release_all, is_active, test-only), led.rs:109,116 (off, pub set_channels); asperitas-measure align.rs:201 (assert_repeatable, test-only), metrics.rs:93 and compare.rs:169 (to_json, test-only). (pins.rs is covered by a separate decision ticket.)

Change: delete them or make them #[cfg(test)].

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The listed items are deleted or cfg(test); workspace builds and tests pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-104 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Deleted pod init() and led off(); made led set_channels private; cfg(test) on knob_override release_all/is_active, align assert_repeatable, Metrics/Comparison to_json. Build, tests, clippy all-targets and pod-hw check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Removed dead public surface in asperitas-pod and asperitas-measure per the listed items.
<!-- SECTION:FINAL_SUMMARY:END -->
