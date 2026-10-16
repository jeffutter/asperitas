---
id: TASK-091
title: >-
  Logging: split asperitas-logging into a logging runtime and a wire-protocol
  crate
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-16 20:38'
labels:
  - review-2026-10
  - logging
  - architecture
  - planned
dependencies:
  - TASK-091.04
priority: high
type: chore
ordinal: 193800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Of 10,492 lines under src/, about 2,600 are logging runtime (lib, usb, defmt_log, panic_handler, led, spin_budget, inbound) and about 7,900 are wire protocol and rig work (frame, console, dump, capture, excerpt, replay, control), with 35% comments. replay.rs is used only by firmware rig.rs; parse_wav, install_frames, BlockAssembler and dump::decode have no firmware users (their users are asperitas-measure and examples), yet they must be no_std and allocation-free; constants are restated (replay.rs:74,95 repeat capture.rs:74,123). The name 'logging' hides this: README:19 and lib.rs:1 call it a logging facade while asperitas-rig and asperitas-measure depend on it for the protocol.

Change: a pure no_std protocol crate (e.g. asperitas-wire: frame, dump grammar, control, excerpt layout/grammar, console bodies, capture geometry); asperitas-logging keeps the runtime; replay.rs moves to firmware or a rig crate; host-only decode/assembly go to a std crate.

DECISIONS NEEDED: crate names, and whether host-only items (BlockAssembler, dump::decode, parse_wav, install_frames, dump_reassemble) move to a std crate or the encoder and decoder must stay in one no_std module.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Crate boundaries as decided; no crate depends on code it only needs for another role
- [ ] #2 All 240 logging tests still pass; firmware builds for every layout
- [ ] #3 Hardware recheck ticket passes (rig capture, panictest, enumeration)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 19b50f6
Umbrella ticket; all work is delegated. Order: TASK-091.01 (replay out) -> TASK-091.02 (asperitas-wire) -> TASK-091.03 (asperitas-host) -> TASK-091.04 (docs/scripts). Each non-trivial child needs its own planning pass. Final: workspace tests (240 logging tests redistributed across crates), firmware builds for every layout, then hardware recheck TASK-120 (@human) covers rig capture, panictest, enumeration. Risks: control<->inbound coupling, crate-private TruncWriter, firmware/ workspace Cargo.lock changes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Names: `asperitas-wire` (pure no_std protocol: frame, dump grammar, control, excerpt layout/grammar, console bodies, capture geometry) and `asperitas-host` (std crate: BlockAssembler, dump::decode, parse_wav, install_frames, the dump_reassemble binary, shared capture loading). `asperitas-logging` keeps the runtime. replay.rs moves to firmware or a rig crate.
<!-- SECTION:NOTES:END -->
