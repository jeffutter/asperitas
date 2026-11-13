---
id: TASK-152
title: >-
  HUMAN: asperitas-live with an instrument on a real line-in, and by ear against
  the Pod running main
status: Backlog
assignee:
  - '@human'
created_date: '2026-11-13T21:10:00Z'
labels:
  - m4
  - live
  - hardware
dependencies: []
references:
  - TASK-145.07
  - docs/reference/asperitas-live.md
  - docs/reference/daisy-pod.md
priority: low
type: task
ordinal: 287700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The two checks TASK-145.07 could not do and the owner deferred when closing it and the M4 umbrella on 2026-11-13. Both need hardware the Mac lacks today: its only input is the built-in microphone, and a TRRS-split cable on the headphone jack is not offered as an input, so `asperitas-live --line-in` has no line source. An audio interface (or any device the Mac lists as an input at line level) would unblock both.

Run `cargo run -p asperitas-live --release -- --line-in --params cutoff_hz=800 gain_db=-6 --buffer 64` (flags in docs/reference/asperitas-live.md), with headphones. `--record out.wav` also writes what was sent to the device. For the Pod comparison, `main` must be flashed again: the board currently runs the rig image from TASK-145.06 (flashing runbook: docs/reference/flashing.md, bootloaded layout).

The owner said the Pod comparison can be assumed to work. That is a decision to skip the check, not evidence; this ticket keeps it available. The live engine is bit-identical to `asperitas-cli process` (TASK-145.05, and a 4 s --record capture matched exactly), and the board matches the same reference on real audio (TASK-145.06), so the open question is only how it sounds side by side.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: an instrument on line-in plays through asperitas-live with latency the owner finds acceptable for playing, and no dropouts over a few minutes (the exit line reports zero underruns and overruns)
- [ ] #2 HUMAN: at matching knob positions the live host and the Pod running main sound the same on the same source
- [ ] #3 HUMAN: any defect heard is filed as its own bug ticket with what was heard and the command used
<!-- AC:END -->
