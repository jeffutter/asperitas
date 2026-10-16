---
id: TASK-091.01
title: 'Wire: move replay.rs out of asperitas-logging into the firmware rig'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:38'
updated_date: '2026-10-16 21:13'
labels:
  - planned
dependencies: []
parent_task_id: TASK-091
priority: high
ordinal: 230800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
replay.rs (ReplayCore, FillClaim, PassReport, ~611 lines + tests/replay_core.rs) is used only by firmware/src/bin/rig.rs. Move it into firmware (lib module or rig-only) and move its tests; remove the restated capture geometry constants (replay.rs:74,95 repeat capture.rs:74,123) by importing from capture. AC: asperitas-logging no longer contains replay; firmware builds for every layout; moved tests pass; docs referencing asperitas_logging::replay updated.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-logging contains no replay module, test or doc reference
- [x] #2 ReplayCore, FillClaim, PassReport and the staging constants live in a no_std crate that firmware/src/bin/rig.rs imports
- [x] #3 Replay geometry constants derive from asperitas_logging::capture, none restated
- [x] #4 Moved tests (former tests/replay_core.rs and the in-module cfg(test)) pass under cargo test --workspace on the host
- [x] #5 firmware builds for every layout (internal and bootloaded, with and without stim-excerpt)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this ticket's commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Replay moved into new host-testable no_std workspace crate crates/asperitas-replay (firmware is workspace-excluded and thumb-only, so tests there could not run on host). Geometry already derived from capture; kept. Verified: cargo test --workspace, clippy --workspace --all-targets, cargo doc, scripts/check-bootloaded-builds.sh (all bootloaded images incl. stim-excerpt), rig log-defmt+stim-excerpt build, fmt. Internal-layout rig does not fit 128K flash (pre-existing, rig is bootloaded-only).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved replay.rs and tests/replay_core.rs out of asperitas-logging into new crate asperitas-replay; firmware rig imports asperitas_replay; doc references fixed.
<!-- SECTION:FINAL_SUMMARY:END -->
