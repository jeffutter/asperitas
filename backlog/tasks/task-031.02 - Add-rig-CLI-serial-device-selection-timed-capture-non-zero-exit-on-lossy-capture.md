---
id: TASK-031.02
title: >-
  Add rig CLI: serial device selection, timed capture, non-zero exit on lossy
  capture
status: To Do
assignee: []
created_date: '2026-10-15 04:26'
updated_date: '2026-10-15 04:26'
labels:
  - task
dependencies:
  - TASK-031.01
parent_task_id: TASK-031
priority: high
ordinal: 153800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add the asperitas-rig binary on top of the capture engine: open the USB CDC console serial port with no manual serial configuration beyond selecting the device (auto-detect or --device; macOS /dev/cu.usbmodem* and Linux /dev/ttyACM*), capture for --duration, write the artifact to --out, print the summary, and exit non-zero when any frame fails integrity or device drops exceed --max-drops. Port opening/selection logic must be testable without a board (inject the reader). Covers AC #1, #2, #4 of TASK-031. Depends on the engine sub-ticket.
<!-- SECTION:DESCRIPTION:END -->
