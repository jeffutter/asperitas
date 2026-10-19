---
id: TASK-141
title: 'Fix: rig capture dump never starts since the base64 crate swap (33f0847)'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 18:20'
updated_date: '2026-10-19 18:39'
labels:
  - firmware
  - hardware
  - regression
  - planned
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
- [x] #1 Root cause identified and recorded in the ticket notes (what fails on the device at dump start, and why host tests missed it).
- [x] #2 A regression test fails on the old behaviour and passes on the fix, using the device's real buffer sizes.
- [x] #3 gates.sh push passes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by b802f1c. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause (AC1): the capture ring is SDRAM mapped as Device memory (see ring.rs), which faults on unaligned multi-byte loads. 33f0847 made dump::encode call base64's STANDARD.encode_slice directly on the ring slice; disassembly of the pre-fix rig ELF shows its internal_encode loading input with ldrd/ldr.w at whatever alignment the slice has, and 129-byte chunks start at odd ring offsets, so the first misaligned chunk faults in thread mode while the SAI interrupt keeps audio running. The old hand-written encoder read byte-wise. The OutputSliceTooSmall theory in the description was wrong. Host tests missed it because host memory permits unaligned access. Fix: dump::encode is now a byte-at-a-time encoder using volatile single-byte loads (cannot be widened or merged); base64 remains the decoder. Post-fix disassembly has no base64 encode symbols and no ldrd/ldm in dump::encode. AC2 note: a host test cannot fault on unaligned access, so the new test console_dump.rs::encoder_matches_the_standard_engine_at_every_length_and_offset pins wire-output equivalence with the base64 engine for all lengths 0..=129 at 8 start offsets; the fault itself is pinned only by the disassembly check above. Not verified on hardware: TASK-138 owns the bench step (flash rig, ASP_RIG_CAPTURE_SECONDS=10, expect DUMPEND).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rig dump stall root-caused to base64's encoder issuing unaligned wide loads against Device-memory SDRAM; replaced dump::encode with a volatile byte-wise encoder (output identical to STANDARD, pinned by a new test), gates.sh push passes, rig cross-build clean. Hardware confirmation left to TASK-138.
<!-- SECTION:FINAL_SUMMARY:END -->
