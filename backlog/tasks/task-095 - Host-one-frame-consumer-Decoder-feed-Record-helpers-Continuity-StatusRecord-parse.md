---
id: TASK-095
title: >-
  Host: one frame-consumer (Decoder::feed, Record helpers, Continuity,
  StatusRecord::parse)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
  - host
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
- [ ] #1 The copies are replaced by the shared consumer in rig, measure and the examples
- [ ] #2 STATUS is parsed by one function next to its formatter
- [ ] #3 No re-encoding of frames to learn their length
<!-- AC:END -->
