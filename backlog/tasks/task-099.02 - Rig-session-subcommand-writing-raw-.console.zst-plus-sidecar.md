---
id: TASK-099.02
title: 'Rig: session subcommand writing raw .console.zst plus sidecar'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-17 03:06'
labels:
  - task
  - planned
dependencies:
  - TASK-099.01
parent_task_id: TASK-099
priority: medium
ordinal: 246800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add 'asperitas-rig session' to crates/asperitas-rig: optional --probe-reset (probe-rs reset with firmware/asperitas-h750.yaml, owner delegated: console side owned, reset optional flag), attach, require fresh BOOT before sending FX, CTL FX, wait for arm, 20/s SET schedule (reuse control.rs schedule and level_to_wire, which refuses out-of-range), always REL on every exit path including errors, stop at DUMPEND, write the raw stream zstd compressed plus a sidecar (fx, schedule, knob, sweep range, device t_ms of first SET ack if the stream cannot supply it). Covers sweep, soak (triangle ramp with optional snaps) and hold modes now in scripts/rig-session.py. Device selected via pick_device, no hardcoded path. AC: unit tests with a fake link covering REL-on-error and schedule; docs/reference/asperitas-rig.md updated.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-099.02. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added asperitas-rig session (crates/asperitas-rig/src/session.rs, wired in main.rs) with sweep/soak/hold motions, BOOT-before-FX guard, REL on every exit path, DUMPEND stop, zstd raw stream plus <out>.session sidecar (asperitas_host::session::Sidecar). The ticket defined no acceptance criteria, so none were ticked; the description's AC (fake-link tests for REL-on-error and schedule, docs updated) are met: 13 session tests plus 2 sidecar tests, docs/reference/asperitas-rig.md has a Bench session section. cargo clippy --workspace --all-targets and cargo test -p asperitas-rig -p asperitas-host are clean. The sidecar also carries arm_t_ms from the first CAPSTAT with delivered>0 as a device-clock anchor for the analyze ticket. --probe-reset and attach are untested without hardware.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New 'asperitas-rig session' subcommand replaces the scripted bench sequence: fresh-BOOT gate, FX, arm wait, timed SET schedule via control::schedule/level_to_wire, guaranteed REL, raw zstd stream and a parseable sidecar. Host-tested with a fake device.
<!-- SECTION:FINAL_SUMMARY:END -->
