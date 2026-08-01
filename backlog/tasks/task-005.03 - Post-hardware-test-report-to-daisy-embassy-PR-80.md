---
id: TASK-005.03
title: 'Post hardware test report to daisy-embassy PR #80'
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 17:41'
labels: []
dependencies:
  - TASK-005.02
  - TASK-011
documentation:
  - docs/reference/rust-daisy-stack.md
parent_task_id: TASK-005
type: docs
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires a human: posting to someone else's PR under your name is your call, not an agent's.

PR #80 is unreviewed and its author is looking for confirmation. An independent test report on identical hardware (Seed3 in a Daisy Pod) is the highest-value, lowest-effort contribution available to this project — it satisfies the 'contribute upstream' stretch goal without writing a driver.

<https://github.com/daisy-embassy/daisy-embassy/pull/80>

Report what TASK-005.02 actually observed. If something did not work, that is even more worth reporting than success.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: test report posted as a comment on daisy-embassy PR #80
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added TASK-011 as a dependency (2026-08-01, via review-pi-work): the hardcoded 32-sample BLOCK_LENGTH in daisy-embassy PR #80 (vs. the 48-sample libDaisy default this project originally assumed) is worth mentioning in the upstream report — TASK-011 documents it in docs/reference/rust-daisy-stack.md first so this ticket can reference it.
<!-- SECTION:NOTES:END -->
