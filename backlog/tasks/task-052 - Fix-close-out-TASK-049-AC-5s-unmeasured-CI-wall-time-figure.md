---
id: TASK-052
title: 'Fix: close out TASK-049 AC #5''s unmeasured CI wall-time figure'
status: To Do
assignee:
  - '@human'
created_date: '2026-09-11 02:46'
updated_date: '2026-09-13 00:12'
labels:
  - review-followup
dependencies:
  - TASK-049
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-049 (backlog/tasks/task-049 - Chore-widen-the-cargo-doc-warnings-as-errors-gate-from-asperitas-logging-to-the-whole-workspace.md, AC #5). AC #5 requires 'CI's observed time on the first widened run' and was checked off ([x]) despite the ticket's own Implementation Notes stating plainly: 'CI wall time is NOT observed and cannot be from here' — main is 61 commits ahead of origin/main, ci.yml only triggers on push/PR (no workflow_dispatch), so the widened doc-links/doc-links-all-features steps have never actually run on GitHub Actions. Correctness axis: an AC was marked satisfied without the evidence it names. This also sidesteps CLAUDE.md's own ticket convention that mixed agent/human work should be split into subtasks rather than checked off with a caveat — pushing and reading Actions output are outward-facing actions an agent must not perform.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: current main (including TASK-049's commit and everything after it) is pushed to origin so ci.yml's widened doc steps actually execute on a GitHub Actions run
- [ ] #2 HUMAN: gh run list -R jeffutter/asperitas --limit 3 identifies that run, and its log (gh run view <run-id> --log or the Actions UI) gives the wall-clock duration for the '=== cargo doc (workspace) ===' and '=== cargo doc (workspace, all features) ===' steps
- [ ] #3 HUMAN: TASK-049's Implementation Notes are updated with the observed CI figures, replacing the paragraph that currently says this is owed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is the Asperitas Rust firmware/host workspace (crates/asperitas-*, firmware/). This ticket is @human-only: pushing to origin and reading a GitHub Actions run are outward-facing actions per CLAUDE.md's ticket convention, and no amount of local cargo doc timing (already recorded on TASK-049) substitutes for the CI-hosted figure this AC actually asks for.

1. Push current main (everything through and after TASK-049's commit f1ef65b) to origin.
2. Run: gh run list -R jeffutter/asperitas --limit 3 -- find the run ci.yml triggered from that push.
3. Run: gh run view <run-id> --log (or open the run in the Actions UI) and locate the two banners '=== cargo doc (workspace) ===' and '=== cargo doc (workspace, all features) ==='; note each step's wall-clock duration.
4. Edit backlog/tasks/task-049*.md's Implementation Notes: replace the paragraph beginning 'CI wall time is NOT observed and cannot be from here' with the two observed durations and the run URL/id.
5. Mark this ticket Done.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:12
---
Heads-up from TASK-060 planning (2026-09-12), same failure class as your TASK-049 AC #5 finding: TASK-060 AC #2 asks for 'the measured wall time recorded' on a new cross-target firmware clippy gate. An agent cannot observe CI's number for it either - ci.yml still triggers only on push/PR to main with no workflow_dispatch, and main is 84 commits ahead of origin/main. So TASK-060's plan requires every figure to be labelled LOCAL (nix develop .#default, aarch64-darwin) and forbids presenting any of them as CI's. When a person next pushes, these steps join the queue behind your two doc-links figures: firmware fmt --check on the firmware workspace (~1 s local warm), and two whole-package cross clippy runs (--bins, -D warnings) whose local figures are 20 s cold standalone / 11 s immediately after CI's release build / ~2 s warm, plus ~1 s for the RTT-only set. Recorded here rather than in a HUMAN subtask under TASK-060 because a @human child would inherit up the tree and hide the remaining agent work while changing nothing about who can push.
---
<!-- COMMENTS:END -->
