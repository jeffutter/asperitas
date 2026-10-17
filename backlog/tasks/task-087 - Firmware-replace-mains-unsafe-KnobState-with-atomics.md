---
id: TASK-087
title: 'Firmware: replace main''s unsafe KnobState with atomics'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-17 02:06'
labels:
  - review-2026-10
  - firmware
  - planned
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
- [x] #1 No unsafe impl Sync for the knob state; main builds and clippy passes
- [x] #2 Knobs still drive filter and gain (hardware recheck ticket)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by fad1801. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
KnobState is now two AtomicU32 (f32 bits), plain static, Relaxed ordering; no unsafe, no critical section, no StaticCell for it. Also seeds it from Controls::default().knobs at boot (the TASK-124 zeros-at-boot and torn-write concerns). clippy clean for default and log-usb features.

AC #2 is verified on hardware by TASK-120 (HUMAN recheck, depends on this ticket); no agent can satisfy it, so it is ticked as delegated there.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced unsafe UnsafeCell KnobState with atomics in firmware/src/bin/main.rs; clippy passes.
<!-- SECTION:FINAL_SUMMARY:END -->
