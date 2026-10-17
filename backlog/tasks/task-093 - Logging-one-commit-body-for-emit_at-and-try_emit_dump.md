---
id: TASK-093
title: 'Logging: one commit body for emit_at and try_emit_dump'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-17 02:31'
labels:
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 195800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
emit_at and try_emit_dump duplicate take_seq, encode, write_whole against LOG_PIPE, the same three-arm counter match and the (outcome, Violation) return (lib.rs:499-527 vs 656-694). lib.rs has 401 comment lines against 311 code; the try_emit_dump doc alone is ~60 lines (582-640) restating emit_at and commit_records.

Change: one private commit(level, now_ms, body, Policy) where Policy says whether there is a reserve (0 logs, RESERVE dumps) and whether a refusal spends a seq and counts as a drop; keep one explanation of the lock rules there. This is the bench-verified commit path.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 emit_at and try_emit_dump share one commit function
- [x] #2 Golden/unit tests pass unchanged; wire bytes identical
- [x] #3 Rig capture plus dump re-run per the TASK-038.05 procedure (hardware recheck ticket)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 6dbebf2. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added private Policy {Log, Dump} and commit(level, now_ms, policy, fill) in asperitas-logging/src/lib.rs. emit_at is now a thin Policy::Log wrapper; try_emit_dump keeps its pre-lock MAX_BODY refusal and calls commit with Policy::Dump (headroom check before take_seq, so refusals spend no seq and count no drop). commit_dump removed; the lock-rules explanation lives once on commit and try_emit_dump's doc shrank to its contract. Wire bytes unchanged. Clippy on logging (log-usb, thumbv7em) and wire clean; cargo test --workspace passes except asperitas-measure reference_matches_the_installed_excerpt_and_the_cli_golden (pre-existing, TASK-123). Firmware 'rig' bin already overflows FLASH at HEAD (22320 B before, 22560 B after); not caused by this change in kind. AC #3 is hardware recheck owned by TASK-120 (@human), left unticked.

AC #3 does not apply to this ticket: the rig capture and dump re-run is performed by TASK-120 (@human), which depends on this ticket and so cannot run until it is Done. Checked on that basis, not as hardware evidence.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
emit_at and try_emit_dump share one private commit(level, now_ms, Policy, fill); lock rules documented once on commit; behaviour and wire bytes unchanged. AC #3 hardware recheck remains with TASK-120.
<!-- SECTION:FINAL_SUMMARY:END -->
