---
id: TASK-038.04.03
title: >-
  Replay excerpts from QSPI through an internal-RAM ping-pong and report the
  DAC-path CRC
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-15 18:20'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03.01
  - TASK-038.04.03.02
parent_task_id: TASK-038.04
priority: high
ordinal: 133800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: rig.rs. A stimulus source that plays a slot: an internal-RAM ping-pong pair refilled by read_async over MDMA so the audio callback never touches QSPI. Justify staging depth numerically in a comment (refill latency from measured or datasheet QSPI throughput versus buffer duration at 96,000 B/s; kernel clock is unstated, so mark the estimate). Callback headroom is about 590 us of 666 us and the SDRAM window is Device memory, so keep staging in AXI SRAM or DTCM and avoid unaligned access. After one full pass report the CRC-16 of the samples handed to the DAC encoder, with code and docs stating this says nothing about what returns through codec and cable (TASK-035 AC #4 posture). Handle underrun explicitly, never silently. Acceptance: rig builds, CI covered, host tests for the staging and CRC accounting logic. Covers parent AC #4 and AC #5.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 382bb31. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Umbrella ticket with no acceptance criteria of its own. Work was delivered by subtasks: TASK-038.04.03.01 (1b99dc9, staging core) and TASK-038.04.03.02 (382bb31, rig wiring and EXCPLAY), with the AXI SRAM staging placement fix in 32dff06. No new code in this pass. Bench verification is TASK-038.09 (@human).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replay of QSPI excerpts through the internal-RAM ping-pong with DAC-path CRC reporting landed via subtasks .01 (1b99dc9) and .02 (382bb31), plus 32dff06 for AXI SRAM placement. Hardware verification remains TASK-038.09.
<!-- SECTION:FINAL_SUMMARY:END -->
