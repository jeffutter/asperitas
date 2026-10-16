---
id: TASK-091.03
title: 'Host: create asperitas-host std crate for host-only decode and assembly'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:38'
updated_date: '2026-10-16 21:57'
labels:
  - task
  - planned
dependencies:
  - TASK-091.02
parent_task_id: TASK-091
priority: high
ordinal: 232800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Per owner decision, create crates/asperitas-host (std): BlockAssembler, dump::decode, parse_wav, install_frames, the dump_reassemble binary (from logging examples), and shared capture loading currently duplicated in asperitas-rig/asperitas-measure. Wire stays no_std and loses these items. Depends on the wire extraction. AC: no no_std/alloc-free constraint remains on host-only code; rig and measure depend on host and wire only; tests moved and passing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-host std crate exists and holds BlockAssembler, dump decode, parse_wav, install_frames, the dump_reassemble binary and the shared capture loader
- [x] #2 asperitas-wire no longer contains those items and stays no_std
- [x] #3 asperitas-rig and asperitas-measure depend on asperitas-host and asperitas-wire only (plus dsp for measure) with the duplicated capture loading removed
- [x] #4 Moved tests pass; cargo test, clippy and cargo doc for the workspace are clean and the firmware workspace still builds
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit titled 'Host: extract asperitas-host std crate for block assembly, WAV parsing, install stream and capture loading' (trailer Task-Id: TASK-091.03). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Created crates/asperitas-host (std): assemble (BlockAssembler, Action/Failure/Tally/Finish/MissingChunks, moved verbatim), excerpt (parse_wav, WavError, install_frames, INSTALL_LEVEL), capture (load_console / load_console_zst returning proved PCM), stream (pump: the read/push/drain/finish loop), and the dump_reassemble and excerpt_stream examples. dump::decode stays in asperitas-wire: the device-side excerpt EXCDATA parser decodes base64 with it, so it is not host-only. To let the host crate read the grammar, wire now publishes the AUDIO/AUDEND templates, field-width constants and BodyReader. pump replaces the duplicated loops in rig capture(), measure's loader and dump_reassemble; its Pumped result keeps the partial byte count on a read error, which dump_reassemble needs for its exit-2 verdict. rig's control.rs wait() loop exits early on a reply, so it keeps its own loop. Tests moved: assembler suite, WAV suite, install-stream/golden tests (with golden files) and measure's synthetic-stream loader tests now live in asperitas-host; wire keeps codec, Installer and grammar tests. dump_reassemble stays an example (not a [[bin]]); scripts/gates.sh, READMEs and the firmware comment now point at asperitas-host. Docs and CLAUDE crate map are left to TASK-091.04. Verified: cargo test/clippy/doc --workspace clean, firmware cargo check ok, dump_reassemble --selftest passes, and the 300 s rig capture still reassembles 879 blocks with no failures.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New std crate asperitas-host holds BlockAssembler, parse_wav, install_frames, the shared capture loader and stream pump, and the dump_reassemble and excerpt_stream examples. asperitas-wire lost those items and stays no_std (dump::decode remains because the device installer needs it). asperitas-rig and asperitas-measure now depend on host and wire (plus dsp for measure) with the duplicated read/decode loop removed. Moved tests pass; workspace test, clippy and doc are clean and the firmware workspace still checks.
<!-- SECTION:FINAL_SUMMARY:END -->
