---
id: TASK-116.02
title: >-
  Docs: extract the DFU and probe flashing runbook into
  docs/reference/flashing.md and investigations into docs/investigations
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 21:12'
updated_date: '2026-10-18 21:25'
labels:
  - docs
  - task
  - planned
dependencies: []
parent_task_id: TASK-116
priority: medium
ordinal: 257800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Move the Flashing the Seed3 / ST-Link probe / unattended probe / bootloaded layout sections (about daisy-seed3.md lines 69-205 and 403-1260) into a terse docs/reference/flashing.md covering DFU and probe procedure only. Move investigations (RTT/D-cache analysis, nRESET, reset-catch, .sram1_bss, rejected flash-freeing routes) into docs/investigations/ with no ticket narration, dropping deep upstream source-line citations and stale measurements. Preserve facts the CLAUDE.md warnings rely on (probe-rs --connect-under-reset, chip strings that wedge the board).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 flashing.md is a terse runbook for DFU and probe
- [x] #2 Investigations live under docs/investigations/ and flashing.md links them
- [x] #3 Other docs and CLAUDE.md links to moved anchors still resolve
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 6b8616e. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sorted the Flashing/probe/bootloader sections of daisy-seed3.md by paragraph. Runbook content (DFU, probe, unattended rules, exit codes, bootloaded flashing) now in docs/reference/flashing.md (253 lines vs about 800 source lines). Investigations in docs/investigations/: rtt-and-dcache.md, nreset-and-reset-catch.md, bootloaded-layout.md, rejected-flash-routes.md, with ticket narration, dates and deep upstream source-line citations dropped. Kept: --connect-under-reset needs flake probe-rs from main, ST-Link plain reset, STM32H7 chip strings that wedge the board. daisy-seed3.md now 203 lines, keeps hardware facts, What each channel loses, host control, SDRAM, QSPI, libDaisy. Links updated in README, CLAUDE.md, rust-daisy-stack, seed3-schematic-search-log, asperitas-rig, upstream doc, Makefile/Cargo.toml/ci/script comments. scripts/check-doc-artifact-names.sh passes (it scans docs/** recursively).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Split the flashing runbook into docs/reference/flashing.md and moved measurements into four docs/investigations/ files; trimmed daisy-seed3.md to hardware facts with a pointer; repointed all cross-references and code comments. Doc artifact-name gate passes.
<!-- SECTION:FINAL_SUMMARY:END -->
