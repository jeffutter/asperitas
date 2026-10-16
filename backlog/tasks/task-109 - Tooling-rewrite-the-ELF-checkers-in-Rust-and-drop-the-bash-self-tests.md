---
id: TASK-109
title: 'Tooling: rewrite the ELF checkers in Rust and drop the bash self-tests'
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - tooling
  - needs-decision
dependencies:
  - TASK-108
priority: low
type: chore
ordinal: 211800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
check-image-load-addresses.sh (1,285 lines), elf-provenance.sh (842), check-elf-staleness.sh (363) and elf-inputs-digest.sh (82), plus three self-test gates (gates.sh:303,315,344) and firmware/build.rs:154 shelling out to bash. Rules R1-R6 caught the 469 MB main.bin (TASK-059) and are worth keeping, but the implementation parses rust-objdump -h columns by position in bash; gates.sh:317-343 lists four parser traps that each made the check pass while checking nothing; it needs bash 5.

Change: a small host crate or xtask using the object crate that reads section types, load addresses and .asp.prov directly and computes the source digest in build.rs; the self-tests become ordinary cargo test.
DECISION NEEDED: is a new workspace crate/xtask for this acceptable?

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every existing rule R1-R6 and the provenance/staleness checks are enforced by the Rust tool, with unit tests replacing the bash self-tests
- [ ] #2 The three self-test gates and the objdump/objcopy dependency are gone
<!-- AC:END -->
