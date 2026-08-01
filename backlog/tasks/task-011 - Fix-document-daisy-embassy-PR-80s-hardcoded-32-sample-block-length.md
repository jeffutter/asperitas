---
id: TASK-011
title: 'Fix: document daisy-embassy PR #80''s hardcoded 32-sample block length'
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 17:41'
labels:
  - review-followup
dependencies:
  - TASK-005.01
documentation:
  - docs/reference/rust-daisy-stack.md
priority: high
type: docs
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-005.01 (backlog/tasks/task-005.01 - Implement-audio-passthrough-against-the-seed3-feature.md, Description and AC #1). TASK-005 and TASK-005.01 both originally targeted "48 kHz with a 48-sample block, matching libDaisy's Pod default" — but the pinned daisy-embassy fork (PR #80, commit 477083b0227d) hardcodes `pub const BLOCK_LENGTH: usize = 32;` in `src/audio.rs`. It is a `const`, not a field on `AudioConfig` (which only exposes `fs: Fs`), so 32-sample blocks are not a choice we can override from this project without patching the vendored fork. TASK-005.01's own record has already been corrected post-review to state the actual delivered behavior (32-sample blocks), but this fact is not yet recorded in docs/reference/rust-daisy-stack.md, which is this project's living reference for daisy-embassy/PR #80 status (Correct axis: a load-bearing ecosystem fact is undocumented, so the next person touching audio timing will have to rediscover it by reading vendored source).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/reference/rust-daisy-stack.md's "Seed3 support status — PR #80" section states that BLOCK_LENGTH is hardcoded to 32 samples in src/audio.rs and is not configurable via AudioConfig, and notes this diverges from libDaisy's 48-sample Pod default
- [ ] #2 The note cites the exact source location (src/audio.rs, BLOCK_LENGTH const) so a reader can re-verify against a future commit of the fork
- [ ] #3 grep -n 'BLOCK_LENGTH' docs/reference/rust-daisy-stack.md returns a match
<!-- AC:END -->
