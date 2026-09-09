---
id: TASK-032
title: >-
  Add host-initiated device control over the console link, including parameter
  override and restart
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:25'
updated_date: '2026-09-09 01:31'
labels: []
dependencies:
  - TASK-030
  - TASK-031
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
- [ ] #1 The host can query firmware version and the reset reason of the current boot, and can request a restart, over the same USB connection already used for logging.
- [ ] #2 The host can place a mapped parameter under host control, drive it across its full range, and release it back to knob control.
- [ ] #3 Host-driven values enter upstream of parameter smoothing, so the smoothing path stays in circuit during measurement; a test shows the difference between injecting upstream of smoothing versus downstream.
- [ ] #4 Knob-driven behaviour is unchanged when host control is not active, covered by existing host-side tests.
- [ ] #5 Control traffic shares the framed transport and cannot corrupt the log stream, covered by a test that interleaves commands with logging.
- [ ] #6 A malformed or unsupported command is answered with an error response and leaves device state unchanged.
- [ ] #7 Firmware boots and logs normally when no host ever sends a command.
- [ ] #8 The command set and framing are documented alongside the transport documentation from TASK-030.
<!-- AC:END -->
