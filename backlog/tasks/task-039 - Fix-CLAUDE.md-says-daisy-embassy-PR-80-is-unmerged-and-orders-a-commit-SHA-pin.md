---
id: TASK-039
title: >-
  Fix: CLAUDE.md says daisy-embassy PR #80 is unmerged and orders a commit-SHA
  pin
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 11:58'
updated_date: '2026-09-09 19:43'
labels: []
dependencies: []
modified_files:
  - CLAUDE.md
priority: medium
type: chore
ordinal: 61500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
docs/reference/rust-daisy-stack.md records PR #80 as merged on 2026-08-01T14:20:54Z, and firmware/Cargo.toml tracks branch = "master" with the seed3 feature. The pointer text in CLAUDE.md still says the PR 'is currently unmerged. Pin to its commit SHA. Time-sensitive; re-check before relying on it.' An agent that follows that instruction literally would pin a nonexistent SHA or downgrade to a feature branch. Found while parking TASK-038, whose planning note 4 already flagged it as outside that ticket's diff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 CLAUDE.md's rust-daisy-stack bullet states PR #80's real status (merged 2026-08-01, per docs/reference/rust-daisy-stack.md line 40) and drops the instruction to pin an unmerged PR's commit SHA.
- [x] #2 The wording keeps the reason the bullet exists — the stack's Seed3 support is upstream and time-sensitive — without asserting a fact that is now false.
- [x] #3 grep -n 'unmerged' CLAUDE.md returns nothing, and no other file in docs/reference/ repeats the stale claim.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Re-verified by a second run (prior run landed 6e23c00 then stopped before the Done flip, and that commit carried no Task-Id/Co-Authored-By trailer). Evidence for each AC, from the files rather than from prose: CLAUDE.md lines 21-24 now read 'merged on 2026-08-01, so the seed3 feature is on master and no commit SHA pin is needed. Time-sensitive; re-check the doc before relying on it' - the merged status matches docs/reference/rust-daisy-stack.md:40 and the bullet still carries why it exists. 'grep -rn unmerged CLAUDE.md docs/reference/' exits 1 with no output; the only 'commit SHA' hit anywhere under docs/reference/ is rust-daisy-stack.md:42, which says no pin is needed. firmware/Cargo.toml:24 confirms the state the bullet now asserts: daisy-embassy branch = master with the seed3 feature.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 19:39
---
Fixed the rust-daisy-stack.md bullet in CLAUDE.md: states PR #80 merged 2026-08-01 and that seed3 is on master with no SHA pin, while keeping the reason the bullet exists (Seed3 support is upstream and time-sensitive; re-check the doc). Verified grep -n unmerged CLAUDE.md returns nothing (exit 1) and no other docs/reference/ file repeats the stale claim (grep for unmerged/PR #80/commit SHA across docs/reference/ only hits the already-correct merged-status text).
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote the rust-daisy-stack.md pointer bullet in CLAUDE.md: daisy-embassy PR #80 is stated as merged 2026-08-01 with the seed3 feature on master and no commit SHA pin needed, replacing the instruction to pin an unmerged PR's SHA. Kept the bullet's reason for existing (Seed3 support upstream, time-sensitive, re-check the doc). All three ACs verified against the files; HEAD commit amended to carry Task-Id/Co-Authored-By trailers.
<!-- SECTION:FINAL_SUMMARY:END -->
