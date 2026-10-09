---
id: TASK-071.02
title: Make elf-check compare the embedded digest and delete the sidecar stamp
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 11:23'
updated_date: '2026-10-09 11:38'
labels:
  - task
  - planned
dependencies:
  - TASK-071.01
parent_task_id: TASK-071
priority: medium
ordinal: 146800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Consumer half of TASK-071 (AC #1, #2, #5, #6). Depends on the embedded digest ticket. Rework firmware/Makefile elf-check to read the digest from .asp.prov and recompute it from the shared input set; refuse on mismatch with the existing remedy; refuse an ELF lacking the digest field with a message naming the missing field and the remedy (not misreported as stale). Remove ELF_STAMP, ELF_INPUTS_SHA256 stamp writing in build-elf, and every reader/writer/doc mention of <binary>.elf-inputs.sha256. Keep the Cargo.lock caveat (no gate passes --locked) or re-justify it.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-071.02. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
elf-check now reads the embedded digest via elf-provenance.sh digest; missing source_digest refuses with a message naming the field and the remedy; mismatch keeps existing message (stamp relabeled embedded). build-elf no longer writes a stamp; ELF_STAMP and sidecar comments removed; Cargo.lock caveat kept. check-elf-staleness.sh minimally adapted with stub PROV/CARGO; full rewrite and docs/reference/daisy-seed3.md (line ~903 sidecar mention) left to TASK-071.03. Verified: selftest, gates.sh commit, real build-elf then elf-check.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the sidecar elf-inputs.sha256 stamp with the digest embedded in .asp.prov in firmware/Makefile; selftest kept green via stubs.
<!-- SECTION:FINAL_SUMMARY:END -->
