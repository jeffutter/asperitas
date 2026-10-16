---
id: TASK-087
title: 'Firmware: replace main''s unsafe KnobState with atomics'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - firmware
dependencies: []
priority: medium
type: chore
ordinal: 189800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
main.rs:86-118 uses unsafe impl Sync for KnobState with a safety comment claiming every access goes through cortex_m::interrupt::free, but knob_poll_task writes at main.rs:240 with no critical section, so the P6 audio interrupt can preempt an 8-byte store (undefined behaviour in Rust's model; in practice a torn pair of knob values).

Change: two AtomicU32 holding f32 bits as a plain static; no unsafe, no critical section, no StaticCell.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No unsafe impl Sync for the knob state; main builds and clippy passes
- [ ] #2 Knobs still drive filter and gain (hardware recheck ticket)
<!-- AC:END -->
