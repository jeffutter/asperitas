---
id: TASK-091
title: >-
  Logging: split asperitas-logging into a logging runtime and a wire-protocol
  crate
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
  - architecture
  - needs-decision
dependencies: []
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
