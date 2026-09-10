---
id: TASK-042
title: >-
  Update CLAUDE.md's summary of daisy-seed3.md now that there are two flashing
  routes
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 03:14'
labels: []
dependencies: []
modified_files:
  - CLAUDE.md
priority: medium
type: docs
ordinal: 71500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CLAUDE.md:12 indexes docs/reference/daisy-seed3.md as covering 'DFU flashing without a debug probe'. TASK-036.04 renamed that heading to 'Flashing the Seed3' and added a full ST-Link section plus 'What each channel loses'. An agent that reads only CLAUDE.md before touching firmware therefore never learns the probe targets exist, which is the exact asymmetry the reference index exists to prevent. Ticketed separately because TASK-036.04's plan says explicitly not to edit CLAUDE.md in passing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The daisy-seed3.md bullet in CLAUDE.md names both flashing routes (DFU over the onboard USB-C, ST-Link probe) and both diagnostic channels (framed USB console, defmt/RTT), in no more than two lines.
- [ ] #2 No command, flag, or chip string is copied into CLAUDE.md; it stays an index and points at docs/reference/daisy-seed3.md for detail.
<!-- AC:END -->
