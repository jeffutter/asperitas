---
id: TASK-031.01
title: >-
  Add asperitas-rig crate with capture engine: decode, summarise, verdict,
  stable artifact
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 04:26'
updated_date: '2026-10-15 04:32'
labels:
  - planned
dependencies: []
parent_task_id: TASK-031
priority: high
ordinal: 152800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create crates/asperitas-rig (workspace member, workspace lints). Pure, transport-agnostic core: feed bytes from any io::Read through asperitas_logging::frame::Decoder, collect records, read device-reported dropped counts from the in-band loss-counter records (see TASK-030.02/030.03 docs), and produce a diffable, committable artifact (stable line-oriented or JSON format, no wall-clock noise in the body) with timestamped records plus a summary: record count, device-reported dropped count, bad_frames, resyncs, discarded_bytes, seq gaps. Include a verdict function: fail on any bad frame or device drops above a configurable threshold. Unit tests replay synthetic streams including truncated and corrupted frames, run with no board. Covers AC #1 (format), #2 (verdict), #3, #5 of TASK-031. Build on, do not duplicate, crates/asperitas-logging/examples/console_decode.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Crate crates/asperitas-rig builds as workspace member with workspace lints
- [x] #2 capture() decodes any io::Read via asperitas_logging::frame::Decoder and returns a Summary with record count, device-reported dropped, bad_frames, resyncs, discarded_bytes, seq gaps
- [x] #3 verdict() fails on any bad frame or device drops above a configurable threshold
- [x] #4 Artifact rendering is deterministic, line-oriented, with no wall-clock content
- [x] #5 Unit tests with synthetic truncated and corrupted streams pass with no board
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 5fdb099. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
New crate crates/asperitas-rig (lib only). capture/verdict/render per plan. Decisions: device_dropped is dropped_full from the last STATUS of each boot, summed across BOOT records; a seq discontinuity at a BOOT record counts as restarts not seq_gaps; any discarded byte (incl. leading garbage) fails; seq gaps reported but not failing. Read errors return Err. 12 unit tests, no board. console_decode untouched; loop duplicated and noted in module docs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas-rig with capture(), verdict(Policy), deterministic render() artifact; tests, clippy, doc, fmt, workspace build all clean.
<!-- SECTION:FINAL_SUMMARY:END -->
