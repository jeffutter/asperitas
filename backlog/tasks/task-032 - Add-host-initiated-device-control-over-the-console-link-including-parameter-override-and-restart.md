---
id: TASK-032
title: >-
  Add host-initiated device control over the console link, including parameter
  override and restart
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 01:25'
updated_date: '2026-10-15 11:49'
labels:
  - planned
dependencies:
  - TASK-030
  - TASK-031
  - TASK-032.01
  - TASK-032.02
  - TASK-032.03
  - TASK-032.04
documentation:
  - docs/reference/daisy-seed3.md
modified_files:
  - crates/asperitas-logging/src/usb.rs
  - firmware/src/bin/podtest.rs
  - firmware/src/bin/main.rs
priority: high
type: feature
ordinal: 42000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With records flowing reliably outbound, the missing half is commands inbound. Without them every capture still needs a person at the bench: to restart the board, to know why it restarted last time, and to move a parameter while a recording runs. Commands are what turn "flash it, then watch a terminal for four minutes" into a script.

Parameter override is the load-bearing command. It lets a host sweep a mapped parameter across its whole range with no hands on the pots, which is what makes the knob-mapping criteria measurable at all rather than listen-and-judge. One constraint must survive implementation: the override belongs at the knob-value boundary, upstream of the smoothing under test. A host that drove the smoothed parameter directly would measure a clean path the product does not actually have, and the resulting measurements would be worthless in exactly the way a gain-staging mistake is worthless — silently wrong rather than obviously broken.

Restart deserves care separately from the others. A board that cannot be restarted from the host cannot be looped unattended, but adding an inbound command channel must never make a faulted board harder to recover than it is today.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The host can query firmware version and the reset reason of the current boot, and can request a restart, over the same USB connection already used for logging.
- [x] #2 The host can place a mapped parameter under host control, drive it across its full range, and release it back to knob control.
- [x] #3 Host-driven values enter upstream of parameter smoothing, so the smoothing path stays in circuit during measurement; a test shows the difference between injecting upstream of smoothing versus downstream.
- [x] #4 Knob-driven behaviour is unchanged when host control is not active, covered by existing host-side tests.
- [x] #5 Control traffic shares the framed transport and cannot corrupt the log stream, covered by a test that interleaves commands with logging.
- [x] #6 A malformed or unsupported command is answered with an error response and leaves device state unchanged.
- [x] #7 Firmware boots and logs normally when no host ever sends a command.
- [x] #8 The command set and framing are documented alongside the transport documentation from TASK-030.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 14d22e6
Approach: all work is delegated to four sub-tasks; the umbrella is a tracking ticket. The inbound CDC OUT path (crates/asperitas-logging/src/inbound.rs) already exists and was designed for this ticket, so no new transport is needed.
.01 protocol codec and verb dispatcher (host-testable), .02 knob-boundary override layer with the upstream-vs-downstream smoothing test (host-testable, covers AC3/AC4), .03 firmware wiring (version, RCC reset reason, safe software restart, override apply), .04 asperitas-rig control subcommands. Order: .01 and .02 in parallel, then .03 and .04.
Restart safety: serviced independent of audio and channel backpressure; BOOT/RESET/DFU entry untouched.
Verification: cargo test --workspace on host; firmware builds via Makefile; hardware evidence is TASK-033.01. AC1 and AC2 are fully met only once .03 and .04 land.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delivered by TASK-032.01 (control.rs codec and verb dispatcher, proptest and adversarial suites), .02 (KnobOverride in asperitas-pod), .03 (firmware wiring: CTL VER/RSN/RST/SET/REL, reset reason from RCC_RSR, build.rs version), .04 (asperitas-rig query/restart/override). Criterion evidence: #1 query verified on hardware 2026-10-15 (podtest, `asperitas-rig query` printed `fw=0.1.0+f3c4f89 reset_reason=power-on`); restart is host-tested only. #2 override set/sweep/release host-tested; not run on a board. #3 knob_boundary_injection_is_smoothed_direct_set_is_not. #4 merge_without_override_is_bit_identical plus the existing suite. #5 write_whole_rounds_are_byte_exact_under_randomized_interleaving and the rig query_skips_interleaved_logs test (replies and logs share the framed transport). #6 control_reaches_control_and_rejects_are_counted_without_effect plus the adversarial command suite. #7 podtest flashed with the control wiring booted and logged a 5 s capture with no host command sent (bad_frames=0, accounting_ok=true, 528 podtest lines; the verdict failed only on pre-attach drops and 32 discarded leading bytes). #8 docs/reference/daisy-seed3.md, section 'Host control channel (firmware wiring)'. Not verified on hardware and left to TASK-033.01: the RST restart and re-enumeration, SET/REL driving a knob on the main binary, and the reset-reason value after a software restart. Observed, unexplained: reset_reason read `power-on` after a probe-rs reset; the RSR priority mapping is host-tested but TASK-033.01 should check what a probe reset reports. Binaries: podtest only was run; main was not flashed.
<!-- SECTION:FINAL_SUMMARY:END -->
