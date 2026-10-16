---
id: TASK-091.02
title: 'Wire: extract asperitas-wire (no_std protocol crate) from asperitas-logging'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:38'
updated_date: '2026-10-16 21:28'
labels:
  - task
  - planned
dependencies:
  - TASK-091.01
parent_task_id: TASK-091
priority: high
ordinal: 231800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Per owner decision, create crates/asperitas-wire: pure no_std, no embassy deps. Move frame, dump grammar (encoder side), console bodies, capture geometry, excerpt layout/grammar, control. Resolve the coupling: control and usb use inbound::InboundRecord, console/panic_handler use the crate-private TruncWriter (move to wire or give wire its own), frame::Decoder feeds inbound. asperitas-logging keeps runtime (lib, usb, defmt_log, panic_handler, led, spin_budget, inbound) and depends on wire. Move the matching tests/golden files. Repoint firmware, asperitas-rig, asperitas-measure, asperitas-dsp, asperitas-pod, scripts. AC: logging depends on wire not vice versa; all 240 logging tests still pass; firmware builds for every layout; workspace gates pass.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit titled 'Wire: extract asperitas-wire no_std protocol crate from asperitas-logging'. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Created crates/asperitas-wire (no_std; only dep is log, for Level). Moved frame, dump, console, capture, excerpt, control plus tests (capture_geometry, console_dump, console_frame, excerpt*, golden) and examples. TruncWriter now pub in wire; InboundRecord moved to wire::inbound (re-exported from logging::inbound; InboundFeed/InboundStats/INBOUND stay in logging). The ring-headroom section of console_dump went to logging/tests/dump_headroom.rs because it needs LOG_PIPE_SIZE. commit_path_no_panic reads wire sources via include_str. Repointed firmware, rig, replay, measure, dsp comments, scripts, gate-costs digest. Test total before and after: 227 (the '240' in the description was approximate). Wire does not depend on logging; gates.sh ci passes (all layouts, clippy, doc, tests). Docs/crate map left to TASK-091.04.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-wire extracted as a no_std protocol crate; asperitas-logging keeps the runtime and depends on it; all consumers repointed; full ci gate tier passes.
<!-- SECTION:FINAL_SUMMARY:END -->
