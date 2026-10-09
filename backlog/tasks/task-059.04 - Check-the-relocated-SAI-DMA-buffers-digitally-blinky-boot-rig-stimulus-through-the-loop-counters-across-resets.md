---
id: TASK-059.04
title: >-
  Check the relocated SAI DMA buffers digitally: blinky boot, rig stimulus
  through the loop, counters across resets
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 20:24'
labels: []
dependencies:
  - TASK-059.01
parent_task_id: TASK-059
priority: medium
type: task
ordinal: 150800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent half of TASK-059.03, split out 2026-10-09 at the owner's request. TASK-059.03 assumed no objective loopback exists; TASK-038.05 has since measured the Pod loop at -0.41 dB at -20 dBFS (docs/reference/daisy-pod.md), so a stimulus through the loop is digital evidence that the SAI DMA buffers at 0x240021b8..0x240025b8 work. What stays in TASK-059.03 is what only a person can judge: the steady-green LED and hearing main. Use the project's probe path only (chip description ASPERITAS_H750IB, never a stock --chip STM32H7 attach). Do not read memory through the probe during a capture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 blinky built from the post-TASK-059.01 tree is flashed over the probe path; the first four bytes of the flashed image are recorded and the board is confirmed alive by a probe-rs attach that reads the vector table and finds a plausible reset handler (an agent cannot see the LED, so the steady-green check stays in TASK-059.03)
- [ ] #2 rig is flashed with FEATURES=seed3 and its stimulus returns through the loop: the console shows BOOT, one RIGCFG record reading icache=0 dcache=0, and STATUS records whose sent= advances with dropped_full=0; the actual counter values are pasted, decoded with console_decode if captured to a file
- [ ] #3 The returned level through the loop matches the expected -0.41 dB at -20 dBFS within a stated tolerance, so the DMA buffers are demonstrably moving samples and not stale data
- [ ] #4 The previous two criteria are repeated across at least two resets or power cycles reached by the probe, and the results are quoted side by side
- [ ] #5 Anything unexpected is filed as a bug ticket carrying the counters or capture position, not checked off
<!-- AC:END -->
