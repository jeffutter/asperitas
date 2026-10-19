---
id: TASK-141
title: 'Fix: rig capture dump never starts since the base64 crate swap (33f0847)'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 18:20'
labels:
  - firmware
  - hardware
  - regression
dependencies: []
priority: high
type: bug
ordinal: 267800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found on the bench during the TASK-138 recheck (2026-10-19). A bootloaded rig image built with ASP_RIG_CAPTURE_SECONDS=10 goes silent on the console at t=12.2 s, when the capture window closes and the dump should begin: the last record is CAPSTAT delivered=29 expected=1, no DUMPEND ever arrives, and STATUS/CAPSTAT stop too. Audio keeps running (the 1 kHz stimulus tone plays forever), consistent with a panic or hang in thread-mode code while the SAI interrupt path lives on. The host only attaches with `cat`; no writes were involved.

Bisected on the board (build rig, ASP_RIG_CAPTURE_SECONDS=10, flash, cat 30 s, look for DUMPEND):
- 63ff25c good (DUMPEND blocks=30, elapsed_ms=2057)
- 092329d good
- 33f0847 BAD ("Logging: use the base64 crate for the dump codec instead of the hand-written one", TASK-098)
- 5a64490 bad (4350 bytes, same stall)

Suspect: crates/asperitas-wire/src/dump.rs encode path now calls STANDARD.encode_slice and turns OutputSliceTooSmall into unreachable!(); the base64 crate may need more output room than encoded_len (or the panic handler cannot report). Not yet root-caused. The host-only tests pass, so they do not cover the device buffer sizes. Impact: every rig dump (TASK-099.05 session, TASK-138 AC2 install, any 300 s capture) is dead on hardware.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause identified and recorded in the ticket notes (what fails on the device at dump start, and why host tests missed it).
- [ ] #2 A regression test fails on the old behaviour and passes on the fix, using the device's real buffer sizes.
- [ ] #3 gates.sh push passes.
<!-- AC:END -->
