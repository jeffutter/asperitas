---
id: TASK-114
title: >-
  Docs: fix probe documentation stale since TASK-037 closed, including chip
  strings that wedge the board
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - docs
dependencies: []
priority: high
type: chore
ordinal: 216800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-037 is Done but text still says the probe has not been verified: CLAUDE.md:18-19, README:259-262, daisy-seed3.md:411-412,476-482,617-618,851,913, rust-daisy-stack.md:116. README:281,286 say the Makefile uses --chip STM32H750IBKx, but firmware/Makefile:89-90 uses ASPERITAS_H750IB plus asperitas-h750.yaml, and daisy-seed3.md:455-464 says the stock STM32H7 entry sets TRACECLKEN and wedges this board's debug port; so the README's DWARF check uses an entry the docs call unsafe. daisy-seed3.md:465 patches this by announcing the strings 'now read ASPERITAS_H750IB' instead of editing them; they are still wrong at 496-497, 515-519, 576 and rust-daisy-stack.md:103. README:259 says probe-rs 0.32.0 from the flake while flake.nix:56-73 builds probe-rs from main at 5214b96 (and its :91 comment says 0.32.0); doc-001:440 cites nix/probe-rs-cortex-m-reset-catch.patch but there is no nix/ directory; docs/upstream/probe-rs-connect-under-reset.md:3 says 'Not reported upstream yet' while daisy-seed3.md:792 says upstream PR #4441 merged 2026-10-11.

Change: edit every chip string and status sentence in place (never 'read X as Y'), correct the version/patch references, mark the upstream write-up resolved with the PR link.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No document says the probe is unverified or uses the stock STM32H750IBKx chip entry for this board
- [ ] #2 Version, patch and upstream-status statements agree with flake.nix and the merged PR
<!-- AC:END -->
