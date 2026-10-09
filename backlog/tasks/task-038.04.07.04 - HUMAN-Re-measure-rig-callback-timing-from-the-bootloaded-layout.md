---
id: TASK-038.04.07.04
title: 'HUMAN: Re-measure rig callback timing from the bootloaded layout'
status: Blocked
assignee:
  - '@human'
created_date: '2026-10-09 02:24'
updated_date: '2026-10-09 10:55'
labels:
  - task
dependencies:
  - TASK-038.04.07.02
parent_task_id: TASK-038.04.07
priority: medium
ordinal: 143800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Code placement changes under the bootloaded layout, so TASK-038.05 bench numbers (max_block_us, worst_gap_us, 72 us callback) predate it. Flash rig bootloaded and re-run the timing measurement; record the new numbers next to the old ones in the rig docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: rig flashed via the bootloader and runs the measurement rig
- [ ] #2 HUMAN: new max_block_us / worst_gap_us recorded beside the TASK-038.05 numbers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.
<!-- SECTION:NOTES:END -->
