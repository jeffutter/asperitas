---
id: TASK-071.03
title: Rewrite the elf-staleness selftest and docs for the embedded digest
status: Done
assignee: []
created_date: '2026-10-09 11:23'
updated_date: '2026-10-09 11:44'
labels:
  - planned
dependencies:
  - TASK-071.02
parent_task_id: TASK-071
priority: medium
ordinal: 147800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verification and docs half of TASK-071 (AC #7, #8, #9). Rework scripts/check-elf-staleness.sh selftest: raw-cargo relink passes, edited input fails, added/removed input fails, ELF without the digest is refused, bulk mtime refresh stays silent, target/ pruned; stays a commit-tier gate. Update the comment block above elf-check in firmware/Makefile, the provenance() doc in firmware/build.rs, docs/reference/daisy-seed3.md (exit-code table row about the missing stamp, etc.), scripts/gates.sh comments. Run scripts/gates.sh commit and push; re-record docs/gate-costs.json if any gate cost moves.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Selftest covers raw-cargo relink pass, edited/added/removed input fail, digest-less ELF refused, bulk mtime silent, target/ pruned, and remains commit-tier
- [x] #2 No stale stamp wording in firmware/Makefile, build.rs, scripts/gates.sh, docs/reference/daisy-seed3.md
- [x] #3 scripts/gates.sh commit green, gate-costs re-recorded if moved, pushed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED in the TASK-071.03 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Selftest audited: stamp-named cases/vars renamed, added raw-cargo-relink-passes and removed-input-detected (now 12 cases, ~1 s). Stale stamp wording removed from Makefile, build.rs, gates.sh, elf-inputs-digest.sh and daisy-seed3.md (exit-code table rows now cite embedded digest / missing source_digest). gates.sh commit green (15 gates); selftest cost unchanged within tolerance, gate-costs not re-recorded.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote the elf-staleness selftest around the embedded .asp.prov digest (raw-cargo relink, edit/add/remove/rename, digestless ELF, bulk mtime, target/ prune, empty set, mtime tripwire) and removed stamp wording from Makefile, build.rs, gates.sh and docs.
<!-- SECTION:FINAL_SUMMARY:END -->
