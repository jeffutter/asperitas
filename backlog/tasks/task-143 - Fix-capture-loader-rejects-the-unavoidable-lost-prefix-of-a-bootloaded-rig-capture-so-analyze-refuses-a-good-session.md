---
id: TASK-143
title: >-
  Fix: capture loader rejects the unavoidable lost prefix of a bootloaded rig
  capture, so analyze refuses a good session
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 20:00'
labels:
  - rig
  - host
  - measure
dependencies: []
priority: medium
type: bug
ordinal: 269800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found on the bench (2026-10-19) rerunning TASK-099.05 after TASK-142. `asperitas-rig session --probe-reset` now works against a bootloaded rig (fx_acked=true sets_sent=600 set_acks=600 rel_acks=1 device_errors=0 dump_end=true, 176 dump blocks proved, first_set_t_ms=8206), but `asperitas-measure analyze` on the resulting .console.zst exits 2: 'capture not fully proved: 1 problems (176 blocks proved): 0 bad frames, 121 discarded bytes'.

Cause: asperitas_host::capture (load) treats any discarded byte as an error, matching dump_reassemble ('a short sample vector that looks fine is the failure this exists to prevent'). On a bootloaded rig the host always attaches mid-stream, so the capture starts with a partial frame: the 121 bytes are the tail of the records lost before USB connected (see TASK-142), at the very start, about 60 s before any dump block. They cannot hide dump data.

Fix to decide and pin with tests: discarded bytes before the first valid frame are the attach prefix and are reported but allowed; a discarded byte after the first valid frame, a bad frame, a refused or abandoned block, or a missing block stay errors. Check dump_reassemble and analyze share the rule and that the report states the prefix length. The session sidecar and committed artifacts stay the raw stream.

Blocks TASK-099.05: the session run is good and committed evidence needs analyze to read it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Discarded bytes before the first valid frame no longer fail load or analyze; the prefix length is reported.
- [ ] #2 A discarded byte after the first valid frame, a bad frame and an incomplete block still fail, each pinned by a test.
- [ ] #3 analyze reads a real bootloaded session capture (the TASK-099.05 sweep) and prints its report.
- [ ] #4 gates.sh push passes.
<!-- AC:END -->
