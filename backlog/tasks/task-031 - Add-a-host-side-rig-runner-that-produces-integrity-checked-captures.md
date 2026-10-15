---
id: TASK-031
title: Add a host-side rig runner that produces integrity-checked captures
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 01:24'
updated_date: '2026-10-15 04:47'
labels:
  - planned
dependencies:
  - TASK-031.01
  - TASK-031.02
  - TASK-031.03
modified_files:
  - crates/asperitas-rig/src/main.rs
  - crates/asperitas-rig/Cargo.toml
  - Cargo.toml
priority: high
type: feature
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Every hardware verification ticket to date has been run by hand at the bench: flash, watch a terminal, paste a log into a ticket. That is why control-surface criteria carry a `HUMAN:` prefix even when the criterion itself is a measurement — TASK-027 asks for nothing but a median interval and a burst check, and it is still marked human because somebody has to sit there for four minutes.

Nothing can be automated until a capture is something a program produces and another program can trust. The prerequisite is TASK-030, which makes records self-verifying; this ticket is the reader side of that contract. It should refuse loudly rather than silently produce a plausible-looking file, because the failure mode this project has already been bitten by is a corrupted capture being read as real data.

This is also the foundation the audio measurement work builds on, so the artifact format matters more than the interface: it should be diffable and committable, since the evidence standard here is a recorded number, not a pass/fail claim.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A host command runs a capture of a requested duration against a connected board and writes an artifact containing timestamped records plus a summary: record count, dropped-record count as reported by the device, and count of frames failing the integrity check.
- [x] #2 The command exits non-zero when any frame fails its integrity check or the device reports drops above a configurable threshold, so a lossy capture cannot be mistaken for a clean one.
- [x] #3 Capture artifacts have a stable structure and are committed alongside hardware verification tickets as reviewable evidence.
- [x] #4 No manual serial configuration beyond selecting the device is needed on macOS or Linux.
- [x] #5 Unit tests replay synthetic byte streams, including deliberately truncated and corrupted frames, and pass with no board attached.
- [x] #6 Usage is documented where the other host tooling is documented.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against c493eb2
Approach: new workspace crate crates/asperitas-rig reusing asperitas_logging::frame::Decoder (already host-testable, same as examples/console_decode.rs). Split: .01 engine+artifact+verdict (pure, tested on synthetic streams), .02 serial CLI with exit codes, .03 docs. Order: .01 -> .02 -> .03. Work is fully delegated to sub-tickets. Final check: cargo test --workspace, cargo clippy, run the CLI against a recorded byte file via the injected reader path; hardware capture is left to TASK-033.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delivered via TASK-031.01 (engine, verdict, deterministic artifact), .02 (serial CLI, exit codes 0/1/2, device auto-detect, --max-drops), .03 (docs/reference/asperitas-rig.md). Verified at close: cargo test --workspace 378 passed, clippy clean, 19 asperitas-rig tests (golden artifact, truncated/corrupted frames, drop threshold, exit codes) pass with no board. CLI run on this host with two boards attached exits 2 and asks for --device, as documented. Not exercised: a real hardware capture (left to TASK-033/.01) and Linux /dev/ttyACM* detection (covered only by unit tests on the name filter).
<!-- SECTION:FINAL_SUMMARY:END -->
