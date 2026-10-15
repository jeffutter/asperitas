---
id: TASK-032.04
title: 'Add control subcommands to asperitas-rig: query, restart, override sweep'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 06:33'
labels:
  - task
  - planned
dependencies:
  - TASK-032.01
parent_task_id: TASK-032
priority: high
ordinal: 158800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the asperitas-rig CLI (crates/asperitas-rig) to send control frames on the same serial device it captures from: query version and reset reason, request a restart and wait for re-enumeration, and hold or sweep a knob override across 0..1 over a timed capture then release. Frames come from the TASK-032 protocol module so host and device share one codec. Host-side unit tests with a fake serial transport; document usage next to the existing rig docs. Out of scope: running on hardware (TASK-033.01).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 5d1feb7. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-rig/src/control.rs (Link request/reply over Read+Write, query, restart with injectable list/open/sleep, schedule, run_override with capture on a scoped thread and unconditional REL). main.rs now has capture|query|restart|override subcommands; the old flat invocation is gone and docs updated (no scripts called it). Deviation from plan: wire range is 0x0000..=0xFFFF per control.rs, not 12-bit. Sweep rate fixed at 20 Hz rather than a flag. 10 new host tests with fake transports; cargo test -p asperitas-rig, clippy --workspace --all-targets and fmt pass. Not run on hardware (TASK-033.01).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-rig gains query, restart and override subcommands (capture moved under its own subcommand) built on the shared asperitas-logging control codec, with host-tested request/reply Link, re-enumeration wait, hold/sweep schedule and guaranteed REL. Docs in docs/reference/asperitas-rig.md.
<!-- SECTION:FINAL_SUMMARY:END -->
