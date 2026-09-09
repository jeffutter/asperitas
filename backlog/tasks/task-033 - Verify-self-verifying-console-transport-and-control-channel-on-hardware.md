---
id: TASK-033
title: Verify self-verifying console transport and control channel on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:25'
updated_date: '2026-09-09 01:31'
labels: []
dependencies:
  - TASK-030
  - TASK-031
  - TASK-032
documentation:
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-030, TASK-031 and TASK-032 are all verifiable by an agent only as far as code and host-side tests. Whether real bytes survive a real USB link under real load, and whether a real board obeys a real command, can only be settled on hardware — and until they are, every ticket downstream rests on an unproven transport.

This needs one person at the bench once. Flashing still requires holding BOOT and tapping RESET by hand until the debug probe path lands (TASK-036, TASK-037), so this cannot be fully remote. After it passes, captures stop needing hands for anything that does not involve turning a knob, which is what unblocks the poll-rate and detent-ratio tickets.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: with the board flashed to the build containing TASK-030 and TASK-032, a capture of at least 240 s taken through the TASK-031 runner reports zero frames failing the integrity check, against the 8.8% line-corruption baseline recorded in TASK-018.04.
- [ ] #2 HUMAN: encoder click and both buttons are pressed several times each during that capture and every press and release appears in it — this reproduces the specific loss that ate two button presses in TASK-018.04's capture.
- [ ] #3 HUMAN: the host requests a restart and the board comes back and re-enumerates with nobody touching BOOT or RESET, and the reset reason reported afterwards identifies a software-initiated restart.
- [ ] #4 HUMAN: the before/after corruption rates are recorded as numbers in this ticket's implementation notes, per this project's convention of recording measured values rather than pass/fail.
- [ ] #5 HUMAN: if any criterion fails, a bug ticket is filed with the offending timestamps from the capture artifact rather than the criteria being checked off.
<!-- AC:END -->
