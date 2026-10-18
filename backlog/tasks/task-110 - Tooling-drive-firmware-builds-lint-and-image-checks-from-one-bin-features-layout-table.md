---
id: TASK-110
title: >-
  Tooling: drive firmware builds, lint and image checks from one (bin, features,
  layout) table
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 21:10'
labels:
  - review-2026-10
  - tooling
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 212800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rig-stim-ess-build and rig-stim-pulse-build (gates.sh:417-421) run exactly the commands of rows 36-37 of the bootloaded matrix (their 0.13 s costs show no-op rebuilds); rig variants account for 8 build gates and 4 clippy gates. firmware-cross-compile lists binaries by hand (--bin main --bin blinky ..., gates.sh:458) while check-image-load-addresses.sh:231 finds them from src/bin/*.rs: a new binary added to src/bin but not to the list makes the load-address gate die with 'no ELF' on a clean runner and may grade a stale ELF locally.

Change: one table that builds, lints and checks each row in a loop; adding a DSP binary or feature becomes a one-row edit.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Duplicate rig gates are gone
- [x] #2 Adding a binary under src/bin requires editing one table only
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by b6300b7. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added scripts/firmware-variants.table (one row per layout/rtt/features/tag/env/bins/actions) and scripts/firmware-variants.sh (build|clippy|grade|list|check, exit 0/1/2, attempts every row). gates.sh now has four firmware gates: firmware-variants-coverage (commit, source-level), firmware-builds (push), image-load-addresses (push), firmware-clippy (commit). Gate counts before: commit 16, push 32, ci 33; after: commit 12, push 19, ci 20. Removed scripts/check-bootloaded-builds.sh; REPLAY AXI-tail check moved into the driver's replay action. Duplicates rig-stim-ess-build and rig-stim-pulse-build are gone (bootloaded rows cover them). Rig clippy rows now run under the bootloaded layout in the same row as their build. Bootloaded rows all gain grade (previously only the matrix rows were graded). AC2 proven with a temporary firmware/src/bin/zz.rs: check, build, grade and clippy all picked it up with no table edit; removed afterwards. gates.sh push: every firmware gate green; only cargo-test fails, on the pre-existing TASK-123.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Firmware builds, clippy and image-load grading are driven from scripts/firmware-variants.table by scripts/firmware-variants.sh. Four gates replace 16 hand-written ones, duplicates removed, a new src/bin file needs no edit, and a coverage check fails if a binary lacks build, clippy or grade coverage.
<!-- SECTION:FINAL_SUMMARY:END -->
