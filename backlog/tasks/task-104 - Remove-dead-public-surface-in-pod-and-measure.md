---
id: TASK-104
title: Remove dead public surface in pod and measure
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - pod
  - measure
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
- [ ] #1 The listed items are deleted or cfg(test); workspace builds and tests pass
<!-- AC:END -->
