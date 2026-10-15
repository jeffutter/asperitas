---
id: TASK-032.01
title: >-
  Define the host control protocol: host-testable command codec and verb
  dispatcher
status: Done
assignee: []
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 06:08'
labels:
  - planned
dependencies: []
parent_task_id: TASK-032
priority: high
ordinal: 155800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a host-testable module in asperitas-logging (default features, no log-usb gate, like frame/inbound) defining the control verbs carried as ordinary v1 frames over the existing inbound path (inbound.rs docs already anticipate this): GET_VERSION, GET_RESET_REASON, RESTART, OVERRIDE_SET(knob, value), OVERRIDE_RELEASE(knob), plus the device-to-host reply records. Includes the verb dispatcher that routes InboundRecord by verb between control and excerpt install (the dispatcher inbound.rs says TASK-032 adds). Unknown verbs and malformed bodies are counted and ignored, never panic. Out of scope: hardware access, knob state, host CLI. Acceptance: property/round-trip tests for every verb; adversarial bodies rejected; excerpt records still reach the excerpt consumer unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Module crates/asperitas-logging/src/control.rs (default features, no log-usb gate) defines Verb/Command (GetVersion, GetResetReason, Restart, OverrideSet{knob,value}, OverrideRelease{knob}) and Reply records, each with encode-to-body and strict parse-from-body
- [x] #2 Property tests (proptest) round-trip every command and reply body, and frame::encode -> Decoder -> InboundRecord -> parse preserves them
- [x] #3 Adversarial bodies (truncated, trailing bytes, bad hex/decimal, out-of-range knob or value, empty, oversized, non-ASCII) return Err and never panic
- [x] #4 dispatch(InboundRecord) routes excerpt bodies to the excerpt consumer byte-for-byte unchanged and control bodies to the control consumer; unknown verbs and malformed control bodies bump a counter and are ignored
- [x] #5 Malformed or unsupported command yields an Error reply value (TASK-032 AC6) without any state change; cargo test --workspace and clippy pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by e861200. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-logging/src/control.rs (ungated, no_std): Command/Reply codecs with strict parsing (CTL and CTLR prefixes), dispatch/route with ControlStats, Error reply values. inbound.rs docs updated. Knob id is opaque u8 and value u16; BadKnob/BadValue codes are reserved for the layer owning the knob table (TASK-032.02/.03). Protocol docs table left to TASK-032.03.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New control module with proptest round trips (incl. frame path), adversarial rejection tests, excerpt pass-through routing test and counters; cargo test --workspace, clippy, fmt and doc pass.
<!-- SECTION:FINAL_SUMMARY:END -->
