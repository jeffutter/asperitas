---
id: TASK-101
title: 'Pod: pure ControlDecoder::step so the hw glue is host-tested'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - pod
dependencies: []
priority: medium
type: chore
ordinal: 203800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
encoder.rs:343-385 holds the active-low inversion, A=bit1/B=bit0 packing, the drain-every-poll rule and the Edge-to-ControlEvent mapping (three copies) inside #[cfg(feature='pod-hw')] code with no host test, while the decoder and debouncer below it have 40 tests.

Change: a pure ControlDecoder::step(PodInputs{a,b,click,b1,b2}) -> [Option<ControlEvent>;4] with host tests; ControlSurface::poll becomes five pin reads. podtest encoder/button behaviour is hardware-verified, so re-run it.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ControlDecoder::step exists with host tests for inversion, packing, drain and edge mapping
- [ ] #2 podtest encoder and button behaviour re-verified (hardware recheck ticket)
<!-- AC:END -->
