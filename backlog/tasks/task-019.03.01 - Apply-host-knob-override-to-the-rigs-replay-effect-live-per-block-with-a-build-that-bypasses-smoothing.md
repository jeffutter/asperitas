---
id: TASK-019.03.01
title: >-
  Apply host knob override to the rig's replay effect live, per block, with a
  build that bypasses smoothing
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 01:20'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-019.03
priority: high
ordinal: 173800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The rig's replay effect (ReplayFx, selected by CTL FX f=,v=) takes one static value and refuses changes once a capture is armed; only firmware/src/bin/main.rs consumes host_control::merge per block. So no existing build can sweep a parameter during a loopback capture. Make rig.rs read the TASK-032 override layer per block (upstream of smoothing, as main.rs does) and drive the selected ReplayFx parameter from it while a capture runs, so asperitas-rig's override sweep moves the effect mid-capture. Also add a cargo feature (name it clearly, e.g. unsmoothed-params) that removes the smoothing so the same rig yields an intentionally step-prone build for the detector sensitivity proof. Smoothing and the override path must be the shipped code in the default build. Add host tests for any pure logic extracted (override-to-param mapping, smoothing bypass). AC: default rig build sweeps fx parameter live during capture; unsmoothed feature build exists and CI/gates build both; bypass fx still bit-exact; rig callback timing estimate stated (measurement is in the HUMAN ticket).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Default rig build applies host override to the selected replay effect every block while a capture is armed, entering upstream of smoothing
- [x] #2 A cargo feature builds rig with smoothing removed; both builds compile in the existing gates
- [x] #3 Host tests cover the extracted pure logic; bypass effect remains bit-exact
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-019.03.01. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ReplayFx::set_knob/new_unsmoothed (+ snap_params on Gain and OnePoleLowPass) in asperitas-dsp; rig.rs gets a rig-local KnobOverride<1> served by CTL SET/REL (accepted mid-capture), merged per block upstream of smoothing; cargo feature unsmoothed-params; gate rig-stim-excerpt-unsmoothed-build; ledger refreshed; docs updated with the callback timing estimate (1-3 us, estimate only). Verified by host tests and gates push tier; no hardware claims.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rig replay effect now follows the host knob override live per block (upstream of smoothing), with an unsmoothed-params feature build gated in CI. Bypass stays bit-exact (tested). Hardware measurement remains TASK-019.03.04.
<!-- SECTION:FINAL_SUMMARY:END -->
