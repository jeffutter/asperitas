---
id: TASK-038.06
title: Document the measurement rig workflow and the SDRAM and QSPI budgets
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:44'
updated_date: '2026-09-09 11:49'
labels: []
dependencies:
  - TASK-038.03
  - TASK-038.04
modified_files:
  - README.md
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038
priority: medium
type: docs
ordinal: 60500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A measurement rig nobody can operate from a page is a rig that gets used once, by the person who wrote it. TASK-030.03 set the precedent for documenting the framed transport; this is the same kind of ticket for the audio side, and it has to land **before** the bench session in TASK-038.05 so that session can be run from the documentation rather than from memory.

Two documents, two different jobs.

The README gains the operational narrative: which stimulus mode to build for which question, how to flash `rig`, how to capture the console to a file while the device is dumping, how to install an excerpt, how to reassemble a dump into samples, and where the resulting artifacts belong. Natural position is a new section between "Debugging Without a Probe" and "Important Hardware Gotchas", matching the existing structure.

`docs/reference/daisy-seed3.md` gains the budgets, which is what the parent's criterion #7 asks for by name: total SDRAM, capture ring size, bytes per second by sample format, capturable seconds, unused headroom; and for QSPI, the excerpt area start, slot stride, slot count, reserved regions, and storage cost per second. Tables, with predicted figures labelled as predictions until TASK-038.05 replaces them with readings.

Both documents have to keep saying which numbers are measured and which are arithmetic. Base64 efficiency and wire cost per second are derivations; erase-dominated install time and full-speed CDC throughput are estimates until someone times them. Blurring that distinction is how a project ends up trusting a phantom measurement.

While writing this down, stale statements surface. Fix them in the same change rather than leaving a trap for the next reader — most notably the claim in TASK-019.03's notes that the firmware image lives in QSPI via execute-in-place, which describes libDaisy C++ builds and not this Rust stack.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A README section placed between "Debugging Without a Probe" and "Important Hardware Gotchas" walks the rig workflow end to end — choosing a stimulus mode, flashing `rig`, capturing the console during a dump, installing an excerpt, reassembling a dump into samples, and where artifacts get committed — so the bench session in TASK-038.05 can be run from the page alone.
- [ ] #2 `docs/reference/daisy-seed3.md` carries the SDRAM budget (total, capture ring size, bytes per second by sample format, capturable seconds, unused headroom) and the QSPI budget (excerpt area start, slot stride, slot count, reserved regions including DaisyBootloader's, bytes per second by format) as tables, with every predicted figure explicitly labelled as a prediction until TASK-038.05 supplies observations.
- [ ] #3 The record grammars this epic adds — `RIGCFG`, `CAPSTAT`, `CAPMAX`, `AUDIO`, `AUDEND`, `EXCSTART`, `EXCDATA`, `EXCEND`, `EXCOK`, `EXCFAIL` — are documented in one place with one worked example line each, beside what TASK-030.03 documents for `BOOT` and `STATUS`, each pointing at the host test that pins it.
- [ ] #4 Every claim is attributed to its source: derived arithmetic versus measured reading versus estimate, covering base64 useful-byte efficiency, wire cost per second of capture, erase-dominated install time, and the still-unmeasured full-speed USB CDC ceiling, so a prediction cannot later be cited as a measurement.
- [ ] #5 Stale statements found while writing are corrected in the same change, specifically TASK-019.03's note that the firmware image occupies QSPI via XIP (this stack links to internal flash and exposes no execute-in-place path) and the parent ticket's "48 kHz mono float" footprint wording, replaced by the 16-bit decision together with the dump-bandwidth reason that produced it.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-038.03 also adds a dump-summary verb emitted when a transfer finishes (blocks, chunks, bytes, elapsed milliseconds, loss counters at that instant), because the rig decides capture boundaries itself and the host learns of completion only from the captured stream. It belongs in criterion #3's grammar list alongside RIGCFG, CAPSTAT, CAPMAX, AUDIO, AUDEND and the EXC verbs.
<!-- SECTION:NOTES:END -->
