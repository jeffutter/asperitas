---
id: TASK-031.02
title: >-
  Add rig CLI: serial device selection, timed capture, non-zero exit on lossy
  capture
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 04:26'
updated_date: '2026-10-15 04:38'
labels:
  - task
  - planned
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Binary asperitas-rig with --device (optional), --duration, --out, --max-drops; auto-detects /dev/cu.usbmodem* (macOS) and /dev/ttyACM* (Linux) when --device omitted, erroring clearly on zero or multiple candidates
- [x] #2 Timed capture implemented as a Read adapter that returns EOF after the deadline, so asperitas_rig::capture is reused unchanged; transient read timeouts do not end the capture early
- [x] #3 Writes render() output to --out, prints summary to stdout, exits 0 on pass and non-zero (distinct from usage/IO errors) on any failed verdict
- [x] #4 Device selection and run logic take an injected reader/path list and are unit/integration tested without a board, including a lossy synthetic stream giving non-zero exit
- [x] #5 cargo test --workspace and cargo clippy --workspace pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 6f2f286. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added crates/asperitas-rig/src/cli.rs (pick_device, list_candidates, Deadline reader, run, Outcome) and src/main.rs (clap, serialport with default-features=false). Exit codes: 0 pass, 1 failed verdict, 2 usage/device/IO error. Binary sets require_status=true so a silent port cannot pass. Tests are board-free. Hardware capture is left to TASK-033.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-rig binary with --device/--duration/--out/--max-drops, port auto-detect, Deadline Read adapter reusing capture unchanged, distinct exit codes, unit tests incl. lossy streams. Workspace tests and clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
