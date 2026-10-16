---
id: TASK-110
title: >-
  Tooling: drive firmware builds, lint and image checks from one (bin, features,
  layout) table
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - tooling
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
- [ ] #1 Duplicate rig gates are gone
- [ ] #2 Adding a binary under src/bin requires editing one table only
<!-- AC:END -->
