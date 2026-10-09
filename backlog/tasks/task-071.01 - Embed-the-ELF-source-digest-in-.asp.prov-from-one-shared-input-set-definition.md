---
id: TASK-071.01
title: Embed the ELF source digest in .asp.prov from one shared input-set definition
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 11:23'
updated_date: '2026-10-09 11:32'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-071
priority: medium
ordinal: 145800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Producer half of TASK-071 (AC #3, #4, and the producer part of #1). Define the digested input set in exactly one place (e.g. a manifest file under firmware/) that both firmware/build.rs and the elf-check side read. build.rs digests those inputs (names and bytes, target/ pruned, same semantics as today's Makefile ELF_INPUTS_SHA256) and writes a source-digest key into the .asp.prov blob; it declares rerun-if-changed for every digested input AND the directories containing them, so a newly added file triggers a relink. Decide how the checker computes a byte-identical digest (single implementation callable from both, not two re-implementations). scripts/elf-provenance.sh must keep ignoring/accepting the new key (selftest unknown-key-ignored) and expose a way to read it. Sidecar stamp stays working until the next ticket. AC: ELF from raw cargo build carries the digest; editing/adding/removing an input relinks; input set defined once.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 build.rs-linked ELF from a raw cargo build carries a source-digest key in .asp.prov
- [x] #2 Digested input set is defined once in firmware/elf-inputs.manifest, read by one script used by both build.rs and the checker
- [x] #3 Editing, adding or removing a digested input relinks (rerun-if-changed on files and containing dirs)
- [x] #4 scripts/elf-provenance.sh show prints the digest; unknown-key-ignored selftest still passes; sidecar stamp still works
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 65f370f. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added firmware/elf-inputs.manifest (single input-set definition) and scripts/elf-inputs-digest.sh (digest/list/paths; one implementation). build.rs runs it, appends source_digest= to .asp.prov and emits rerun-if-changed for every input file and containing directory plus the manifest and script. Makefile ELF_INPUTS/ELF_INPUTS_SHA256 now delegate to the script; digest verified byte-identical to the old Makefile output. elf-provenance.sh parses source_digest (optional), show prints it, new digest subcommand, new selftest case. Verified: raw cargo build embeds digest equal to the script's; adding/removing a src file relinked and changed the digest. gates.sh commit passes; check-elf-staleness selftest passes (sidecar stamp untouched).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Producer half of TASK-071: ELF built by raw cargo now carries source_digest in .asp.prov from a shared manifest read by one script; relink triggers cover files and directories; elf-provenance.sh show/digest expose it.
<!-- SECTION:FINAL_SUMMARY:END -->
