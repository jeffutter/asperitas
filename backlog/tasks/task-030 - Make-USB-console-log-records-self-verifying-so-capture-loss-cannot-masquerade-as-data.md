---
id: TASK-030
title: >-
  Make USB console log records self-verifying so capture loss cannot masquerade
  as data
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-09-09 01:23'
updated_date: '2026-09-09 02:26'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-seed3.md
  - TASK-018.04
  - TASK-027
modified_files:
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/usb.rs
priority: high
type: feature
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Hardware captures are this project's only verification mechanism for control-surface behaviour, and the transport corrupts roughly 8.8% of them (1226 of 13968 lines directly counted in TASK-018.04's notes). That defect nearly produced a false finding: a line reading r2=298 was a record cut mid-number, and it was close to being reported as an ADC glitch.

Two faults in asperitas-logging explain it. First, the pipe write path treats a partial write as success — when the 512-byte buffer is nearly full, a record is written only as far as space remains and the remainder is discarded, which is exactly the observed symptom of lines truncated mid-token. Second, the pipe's mutex is the no-op kind, providing no mutual exclusion, so concurrent producers share the format buffer and the queue's internal counters unprotected.

The deeper problem is that loss is undetectable by the reader. Nothing on the wire distinguishes a complete record from a truncated one, so a consumer parsing this stream cannot tell clean data from garbage. Every planned automated capture is built on these bytes, so integrity has to become something the reader can check rather than something it hopes for. Lost data must surface as an explicit count instead of looking like plausible readings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Log records are self-delimiting and carry their own integrity check, so a reader can distinguish a complete record from a damaged one.
- [ ] #2 A record that does not fit the buffer is dropped whole, never partially; bytes buffered for other records remain valid.
- [ ] #3 Concurrent producers cannot interleave inside a single record.
- [ ] #4 Totals for dropped records and failed frames are reported to the host in-band, so a capture declares its own loss instead of appearing clean.
- [ ] #5 Host-side tests cover truncation at every boundary between frames, corruption inside a frame, and interleaved multi-producer emission — each is detected and never mistaken for a valid record.
- [ ] #6 Existing log call sites in firmware/src/bin/*.rs need no changes.
- [ ] #7 docs/reference/daisy-seed3.md's probe-free debug channel section describes the framing and the loss counters.
<!-- AC:END -->
