---
id: TASK-038.04.06
title: >-
  Document excerpt storage layout, install and replay costs with predicted and
  observed columns
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-15 18:27'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03
parent_task_id: TASK-038.04
priority: high
ordinal: 136800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: docs/reference/daisy-seed3.md mirror of the layout constants (AC #1 doc half), the 96,000 B/s budget, install wall-clock including erases, replay margin, the rule that the blocking flash API is forbidden in this binary, and the DAC-path CRC posture. Observed columns stay marked pending until TASK-038.09 measures them on the bench. Also correct TASK-019.03's note that QSPI carries the firmware image via XIP (not true of this Rust stack, which links to internal flash). Coordinate wording with TASK-038.06. Covers parent AC #1 and AC #6.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 docs/reference/daisy-seed3.md gains one section 'Instrument excerpts in QSPI' with a layout table whose values are copied from the excerpt module constants (area start 0x100000, stride 0x80000, 14 slots, header sector, reserved top sector, bootloader region below 0x40000 untouched) and a note naming the module as the owner; a host test or doc-consistency check fails if the table and the constants drift
- [x] #2 A cost table with predicted and observed columns: 96,000 B/s mono 16-bit, bytes and sectors per corpus clip, install wall-clock (47 sector erases x 300 ms datasheet max for a 190 kB clip, with the per-page program term), replay staging margin, wire bytes for the install stream; every observed cell reads 'pending TASK-038.09' and every predicted cell states its derivation
- [x] #3 States the rules: only the async flash API may be used in rig (blocking wait_for_write is an unbounded busy loop; async timeouts panic), writes are whole sector-aligned 4096-byte sectors, top sector reserved because the driver asserts against 0x7FFFFF
- [x] #4 States the DAC-path CRC posture: the replay CRC equals the host file CRC and proves only what was handed to the DAC encoder, nothing about the codec or cable, matching TASK-035 AC #4; documents the EXCSTART/EXCDATA/EXCEND/EXCOK/EXCFAIL/EXCPLAY grammar with one worked line each, each pointing at the host test that pins it
- [x] #5 TASK-019.03's 'QSPI also carries the firmware image via XIP' note is corrected with a dated correction (this stack links at 0x08000000 and has no execute-in-place path), unless TASK-038.06 already did so; nothing here duplicates the README workflow or SDRAM budget that TASK-038.06 owns
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 38084e7. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Nothing of this existed at HEAD 0e12f7e. Added section 'Instrument excerpts in QSPI' to docs/reference/daisy-seed3.md (layout, rules, cost table with predicted/observed columns, CRC posture, record grammar with pinning tests), crates/asperitas-logging/tests/excerpt_docs.rs as the drift guard (reads the doc, asserts the section quotes the excerpt constants), and a dated correction in TASK-019.03. Observed cells all read 'pending TASK-038.09'. Datasheet erase/program figures are as quoted in the ticket plan; no datasheet is in-repo. TASK-038.06 had not corrected the XIP note, so it was done here. The correction also notes the bootloaded layout copies from QSPI to AXI SRAM, which is not XIP.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Documented excerpt QSPI layout, rules, predicted-vs-observed cost table, DAC-path CRC posture and record grammar in docs/reference/daisy-seed3.md; added excerpt_docs.rs drift test; corrected TASK-019.03's XIP note. No firmware change.
<!-- SECTION:FINAL_SUMMARY:END -->
