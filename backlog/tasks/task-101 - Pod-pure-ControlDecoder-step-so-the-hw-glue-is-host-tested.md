---
id: TASK-101
title: 'Pod: pure ControlDecoder::step so the hw glue is host-tested'
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 20:11'
labels:
  - review-2026-10
  - pod
  - planned
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
- [x] #1 ControlDecoder::step exists with host tests for inversion, packing, drain and edge mapping
- [ ] #2 podtest encoder and button behaviour re-verified (hardware recheck ticket)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 95a1389

Approach: move the decode logic out of the pod-hw cfg block into a pure, host-testable ControlDecoder in crates/asperitas-pod/src/encoder.rs; ControlSurface::poll becomes five pin reads feeding it.

Steps:
1. In encoder.rs (outside mod hw) add pub struct PodInputs { a_level_high, b_level_high, click_level_high, b1_level_high, b2_level_high: bool } carrying RAW pin levels (so the active-low inversion lives in the tested code), or a/b/click/b1/b2 with documented raw-level semantics.
2. Add pub struct ControlDecoder { encoder: EncoderDecoder, click/button1/button2: DebouncedSwitch } with new() (switches start released), step(&mut self, PodInputs) -> [Option<ControlEvent>; 4] (slot 0 encoder delta only when drain_detents != 0, then click, b1, b2), and state() delegating to encoder.state(). Single private helper maps Edge to Press/Release ControlEvent per switch, replacing the three copies.
3. In hw::ControlSurface replace the four decoder fields with one ControlDecoder; poll reads five pins (is_high), builds PodInputs, calls step, and forwards each Some event to the callback in order. Keep encoder_state() delegating.
4. Host tests in the tests module: active-low inversion (high = released, low = pressed for each of the 5 inputs); A=bit1/B=bit0 packing (check via state() and a known quadrature sequence yielding +/- detent direction); drain every step (sub-detent residue retained, no zero-delta event emitted, delta emitted exactly on the completing step); edge mapping for click/b1/b2 press and release incl. debounce behaviour; event ordering within the array.
5. Verify: cargo test -p asperitas-pod; cargo clippy and a firmware build with --features pod-hw (and any podtest binary build) to confirm the hw glue still compiles; grep callers of ControlSurface::poll/encoder_state for unchanged signatures.

Final testing: host tests pass, pod-hw build passes. AC#2 (hardware recheck of podtest encoder/button) is covered by dependent TASK-120 (HUMAN); the agent cannot satisfy it, so leave AC#2 unticked and note it. Risk: AC#2 is a HUMAN criterion on an @agent ticket; execution should complete AC#1 and not mark Done unattended if the loop treats AC#2 as blocking.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added PodInputs (raw pin levels) and ControlDecoder::step in encoder.rs, outside the pod-hw cfg. ControlSurface::poll is now five is_high reads plus forwarding. 8 new host tests cover inversion, A=bit1/B=bit0 packing, drain/residue, edge mapping and slot order. cargo test -p asperitas-pod (48 pass), clippy and firmware cargo check (pod-hw) pass. AC#2 is a hardware recheck, left unchecked; covered by TASK-120 (HUMAN). Ticket left In Progress, not Done, because an unattended run cannot satisify a HUMAN criterion.
<!-- SECTION:NOTES:END -->
