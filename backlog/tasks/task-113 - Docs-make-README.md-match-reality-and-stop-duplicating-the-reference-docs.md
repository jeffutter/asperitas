---
id: TASK-113
title: 'Docs: make README.md match reality and stop duplicating the reference docs'
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
ordinal: 215800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
README.md:9-21,19,155-163,467,473-489 describes things that do not exist: `cargo run --bin asperitas-cli -- live --processor <name>` 'uses cpal' (the CLI has only Process and Generate, crates/asperitas-cli/src/main.rs:19-24, and no crate depends on cpal; generate is never mentioned); a layout tree of 3 crates and 4 bins when there are 6 crates and 6 bins and audio/, captures/, scripts/, backlog/ are missing; 'stderr on host' logging (lib.rs:12 says no-op); 'patch the vendored fork' (there is none, firmware/Cargo.toml:117 is a git dependency on master); the Reference list omits asperitas-rig.md, seed3-schematic-search-log.md and docs/upstream/. It contradicts itself on hardware status: README:148-149 says bootloaded main/rig are not yet verified (TASK-038.04.07.04) while :437-440 tables a bootloaded 300 s rig run and the ticket is Done; :452 'Not yet measured' sweep/excerpt while :361-362 records excerpt replay exercised (TASK-038.08, .04, .05 Done); slot range written 0..14 (:352) and 0..13 (:368) while SLOT_COUNT=14 (excerpt.rs:115). The DFU flow (102-128), the probe section (257-315) and the rig workflow/bench readings (317-457) duplicate daisy-seed3.md and asperitas-rig.md, which is how the contradictions happened.

Change: rebuild the tree from git ls-files, delete the live section until M4 ships, document generate, replace ticket-owned status sentences with 'see TASK-x', write ranges one way, and cut the README to ~120 lines (pitch, layout, nix shell, blinky, DSP iteration loop, doc index), moving the rig workflow into asperitas-rig.md.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README commands all run as written; the layout tree matches git ls-files
- [ ] #2 No status sentences that a ticket owns; the duplicated sections are removed or point at the reference docs
<!-- AC:END -->
