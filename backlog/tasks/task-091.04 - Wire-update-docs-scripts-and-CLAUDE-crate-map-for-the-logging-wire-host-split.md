---
id: TASK-091.04
title: >-
  Wire: update docs, scripts and CLAUDE crate map for the logging/wire/host
  split
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 20:38'
updated_date: '2026-10-17 02:10'
labels:
  - task
  - planned
dependencies:
  - TASK-091.03
parent_task_id: TASK-091
priority: medium
ordinal: 233800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Fix README:19 and lib.rs:1 wording (logging is no longer called a protocol facade), docs/reference/*.md, scripts/gates.sh, elf-provenance.sh, check-image-load-addresses.sh and lefthook for new crate names and paths. Comments describe current state only. AC: no stale references to moved modules; gates run clean.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-091.04. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Repointed docs/reference (daisy-seed3, asperitas-rig) to asperitas-wire/asperitas-host paths, golden fixture and ctl_frame example homes; README and asperitas-logging lib.rs now describe logging as the device runtime; elf-provenance.sh dropped a stale line-range citation. gates.sh needed no change (workspace gates cover wire/host; dump_reassemble selftest already -p asperitas-host). scripts/gates.sh commit tier passes; check-image-load-addresses passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Docs, README, logging lib.rs wording and elf-provenance.sh comment updated for the logging/wire/host split; commit gates pass.
<!-- SECTION:FINAL_SUMMARY:END -->
