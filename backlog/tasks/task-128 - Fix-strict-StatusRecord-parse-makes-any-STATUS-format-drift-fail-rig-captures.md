---
id: TASK-128
title: >-
  Fix: strict StatusRecord::parse makes any STATUS format drift fail rig
  captures
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 03:09'
updated_date: '2026-10-19 13:47'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 251800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-rig/src/lib.rs now uses strict StatusRecord::parse; a reordered/added field or truncated body bumps unparsed_status, which fails the verdict, and device_dropped loses its contribution. The old parser only needed proto=1 and dropped_full. Also dump_reassemble.rs note() silently omits STATUS lines that fail strict parse. Make dropped_full/ep_err reads tolerant, and surface unparsed STATUS lines in the manifest instead of hiding them.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d01a479. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added StatusRecord::counter (tolerant lookup, wire), rig Accumulator falls back to it on strict parse failure so device_dropped survives drift, manifest gains first_unparsed_status, dump_reassemble note() echoes unparsed STATUS lines. ep_err read is available via counter(); verdict only needs dropped_full.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Strict parse kept; tolerant counter read keeps device_dropped through STATUS drift, and unparsed STATUS bodies are surfaced in the rig manifest and dump_reassemble output. Tests added in wire and rig.
<!-- SECTION:FINAL_SUMMARY:END -->
