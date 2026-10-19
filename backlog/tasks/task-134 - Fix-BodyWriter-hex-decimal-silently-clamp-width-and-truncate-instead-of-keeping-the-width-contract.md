---
id: TASK-134
title: >-
  Fix: BodyWriter hex/decimal silently clamp width and truncate instead of
  keeping the width contract
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 23:08'
updated_date: '2026-10-19 14:20'
labels:
  - planned
dependencies: []
ordinal: 259800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-wire/src/body.rs BodyWriter::hex and decimal clamp digits to 10 and put/truncate silently; audio_body, exc_start_body and the wire_verb! macro (console.rs) rely on silent truncation, and Slot::WIDTH / Why::WIDTH are hand-stated constants not tied to the value types. A wrong width or budget now yields a malformed-but-valid-length body detected only by the receiver. Add debug_assert (or type-level width) on the width contract, and tie the WIDTH consts to the real types with a test so a longer spelling fails the build.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 BodyWriter::hex and decimal debug_assert that digits <= 10 and the value fits the width, with should_panic tests
- [x] #2 WireField::WIDTH consts in console.rs are derived from the real value types (not hand-stated) and a longer spelling or wider type changes the width
- [x] #3 A test renders each field kind and every verb at saturated values and asserts the rendered length equals the declared WIDTH/WORST (including EXCPLAYFAIL)
- [x] #4 cargo test, clippy, fmt, and firmware make check pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by a2fdb18. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
BodyWriter::hex/decimal debug_assert width <= 10 and value fits (release keeps the clamp fallback); should_panic tests added. install.rs EXCOK decimal width now uses excerpt::BYTES_DEC_DIGITS (already const-checked against PCM_CAPACITY). console.rs WireField widths derive from decimal_digits(type max), u16::BITS/4 and longest() over PlayRefusal/MonoLane SPELLINGS; tests cover each field kind, spelling table coverage, and EXCPLAYFAIL saturation. cargo test/clippy/fmt and firmware make check pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Width contract is now asserted in BodyWriter (debug) and WIDTH consts derive from real types, with saturation tests per field kind and EXCPLAYFAIL.
<!-- SECTION:FINAL_SUMMARY:END -->
