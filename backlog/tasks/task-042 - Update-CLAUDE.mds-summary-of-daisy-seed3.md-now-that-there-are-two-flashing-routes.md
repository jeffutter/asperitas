---
id: TASK-042
title: >-
  Update CLAUDE.md's summary of daisy-seed3.md now that there are two flashing
  routes
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 03:14'
updated_date: '2026-09-10 06:46'
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
- [x] #1 The daisy-seed3.md bullet in CLAUDE.md names both flashing routes (DFU over the onboard USB-C, ST-Link probe) and both diagnostic channels (framed USB console, defmt/RTT), in no more than two lines.
- [x] #2 No command, flag, or chip string is copied into CLAUDE.md; it stays an index and points at docs/reference/daisy-seed3.md for detail.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Rewrote only the daisy-seed3.md bullet in CLAUDE.md (lines 11-20). It now names both flashing routes (DFU over the onboard USB-C; ST-Link probe on the SWD pads) and both log channels (framed USB console; defmt/RTT through the probe), with the naming clause spanning exactly two lines. Kept the codec-strapped and libDaisy facts that were already there, and added the two traps the reference documents: the probe is the only view of a board that never enumerates USB, but its capability claims are unmeasured until TASK-037, and RTT keeps no loss ledger so a console drop count does not describe an RTT capture. Verified AC #2 by grepping CLAUDE.md for dfu-util|probe-rs|--chip|STM32H750|make flash|make probe|cargo |log-defmt|PROBE_EXTRA|0x08000000 - no matches, so the bullet stays an index. Prose region still wraps at <=88 columns like the rest of the file.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
CLAUDE.md's daisy-seed3.md index bullet now names both flashing routes (DFU over the onboard USB-C, ST-Link probe on the SWD pads) and both diagnostic channels (framed USB console, defmt/RTT), so an agent reading only CLAUDE.md learns the probe targets exist. No command, flag, or chip string copied - the bullet stays a pointer to docs/reference/daisy-seed3.md.
<!-- SECTION:FINAL_SUMMARY:END -->
