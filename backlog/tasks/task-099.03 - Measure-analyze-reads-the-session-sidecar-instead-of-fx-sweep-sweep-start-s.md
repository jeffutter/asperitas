---
id: TASK-099.03
title: >-
  Measure: analyze reads the session sidecar instead of
  --fx/--sweep/--sweep-start-s
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 02:53'
updated_date: '2026-10-18 20:04'
labels:
  - task
  - planned
dependencies:
  - TASK-099.02
parent_task_id: TASK-099
priority: medium
ordinal: 247800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
asperitas-measure analyze takes a session capture and reads its sidecar for fx, schedule and sweep start, replacing the hand-set flags in cli.rs (keep explicit flags only as overrides if useful, otherwise remove). Fix measure docs that claim captures come from 'asperitas-rig override --sweep'. AC: analyzing a rig session output needs no hand-set flags; tests with a sidecar fixture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 analyzing a rig session output (raw stream plus sidecar) needs no hand-set flags
- [x] #2 measure docs and module docs no longer claim captures come from 'asperitas-rig override --sweep'
- [x] #3 tests use a sidecar fixture covering sweep, soak, excerpt and missing-sidecar cases
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-099.03. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
analyze resolves its mode from the sidecar beside the capture (asperitas_host::session::sidecar_path, moved from rig). Sidecar::sweep_start_s prefers the device-clock span (wrapped mod 1e8 ms), else start_after_arm_s. Removed --fx, --fx-knob, --sweep, --sweep-kind, --soak, --sweep-start-s, --sweep-duration-s; kept --stimulus, --excerpt, --expect-fail. Flags contradicting a sidecar exit 2. Excerpt with a sidecar needs a hold session (fx from fx_f, knob level from sidecar). Without a sidecar the old steady-stimulus default remains. Pre-existing failure: compare::reference_matches_the_installed_excerpt_and_the_cli_golden (TASK-123), unrelated.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Measure analyze now takes fx, knob, motion, range and start from the session sidecar with no hand-set flags; docs fixed; sidecar-fixture tests cover sweep (gain, lowpass), soak, hold, excerpt, missing and bad sidecar and flag conflicts.
<!-- SECTION:FINAL_SUMMARY:END -->
