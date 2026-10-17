---
id: TASK-095
title: >-
  Host: one frame-consumer (Decoder::feed, Record helpers, Continuity,
  StatusRecord::parse)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-17 02:51'
labels:
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 197800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The push/drain/assemble loop is copied in dump_reassemble.rs:272-300, asperitas-measure capture.rs:83-130 (header says it 'deliberately mirrors' the example), asperitas-rig lib.rs:150-196, console_decode.rs:76 and asperitas-rig control.rs:166-215. Record.level is a raw u8 so rig lib.rs:256-267 turns the letter back into a Level and re-encodes the whole frame just to get its length; sequence-continuity tracking is written twice (rig lib.rs:223-231, console_decode.rs:215-236); STATUS is formatted in logging console.rs:212 and hand-parsed in rig lib.rs:270 and dump_reassemble.rs:396. None of it needs std.

Change: add Decoder::feed(&[u8], impl FnMut(Record)), a framed_bytes counter in Stats so the accounting law is a Decoder property, Record::framed_len()/level(), a Continuity type and StatusRecord::parse next to its formatter; move dump_reassemble (CI runs it, gates.sh:378) into a host crate binary sharing one load-capture function with asperitas-measure.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The copies are replaced by the shared consumer in rig, measure and the examples
- [x] #2 STATUS is parsed by one function next to its formatter
- [x] #3 No re-encoding of frames to learn their length
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit 'Wire/host: one frame consumer (Decoder::feed, Record helpers, Continuity, StatusRecord::parse)' (Task-Id: TASK-095). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added Decoder::feed/accounted, Stats.framed_bytes, Record::level/framed_len (wire frame.rs); StatusRecord::parse, is_boot and Continuity (wire console.rs). Rig lib.rs/control.rs, host pump, console_decode and dump_reassemble now use them; the re-encode length helpers, both Continuity copies and both STATUS hand-parses are gone. dump_reassemble moved to a host bin (gates.sh and docs updated). The measure capture loop already went through asperitas_host::capture. StatusRecord::parse is strict (proto=1, all seven fields in order). rig control wait() now drops records after the accepted one in a chunk (previously the unpushed bytes were dropped). Unrelated pre-existing failure: asperitas-measure compare reference test (TASK-123).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
One frame consumer in asperitas-wire (Decoder::feed, framed_bytes accounting, Record helpers, Continuity, StatusRecord::parse) replaces the copied push/drain loops, length re-encoding, continuity tracking and STATUS parsing in rig, host, console_decode and dump_reassemble; dump_reassemble is now a host bin.
<!-- SECTION:FINAL_SUMMARY:END -->
