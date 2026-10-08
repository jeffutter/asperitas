---
id: TASK-069
title: >-
  Decide whether elf-check's freshness record belongs inside the image rather
  than beside it
status: To Do
assignee:
  - '@human'
created_date: '2026-09-14 05:50'
updated_date: '2026-10-08 01:01'
labels: []
dependencies:
  - TASK-056
priority: medium
type: chore
ordinal: 119800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-056 records a content digest of the ELF's inputs in a stamp file beside `release/<binary>`, written by `make build-elf`, and compares it in `elf-check`. That kills the false red a bulk mtime refresh causes today, but the stamp can only ever describe the last link that went through make. Everything that relinks the same path without it leaves the stamp describing an earlier link: both raw-cargo cross-builds in scripts/gates.sh:262,269, and `make build` itself, because cargo objcopy compiles before it objcopies. Bounded consequence: edit a source, run the gates without running make build-elf, and elf-check goes red against an ELF that is actually current. The printed remedy fixes it.

The alternative is to put the digest where TASK-062.01 already puts the cfg set: a src_digest= line in the .asp.prov note that firmware/build.rs stamps, which travels with the artifact and is therefore immune to who built it. TASK-062.01 rejected exactly this argument for provenance (firmware/build.rs:38-45: no host-side record can describe what the name currently holds, because the gates run raw cargo, never make). TASK-056 accepts that limitation for a source stamp specifically, which is why its AC #3 asks for the mechanism in firmware/Makefile and nothing in build.rs.

This is an owner call, not an agent call: it contradicts an acceptance criterion Jeff wrote, and it extends the .asp.prov schema that TASK-062.01 owns. Facts already measured, so nobody re-researches them: elf-provenance.sh:120-146 ignores unknown blob keys by design and asserts it (selftest case unknown-key-ignored), so adding a field needs no reader change; .asp.prov is non-allocated (.section .asp.prov, "", %note, LMA/VMA 0, blank objdump Type), so growing the 57-byte blob by ~77 bytes adds nothing to any .bin and is invisible to all three rules of check-image-load-addresses.sh; no new dependency is needed because build.rs can shell out to the same dual-spelled hasher elf-provenance.sh:505-510 uses. The cost is elsewhere: firmware/build.rs:33-36 opts out of cargo's default change detection, so the digest-in-blob route means emitting rerun-if-changed for all 52 inputs, keeping a Rust enumeration in lockstep with the shell expression, and computing the digest before the link instead of after it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Choose one, and record it as a comment here: keep the sidecar stamp as elf-check's freshness record, or move the digest into .asp.prov so a raw-cargo rebuild carries it too.
- [ ] #2 HUMAN: If the choice is to move it, say whether covering make build's objcopy route is part of the requirement, since that question is what forces production into build.rs rather than into any recipe.
- [ ] #3 HUMAN: If the choice is to keep the sidecar, say explicitly that the narrow false red is accepted (source edited, gates run, no make build-elf in between), so the next reader does not file it again as a bug.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-07: Blocked -> To Do at the owner's request. Its only dependency, TASK-056, is Done. What remains is the owner's decision (AC #1-#3), not another ticket. Still @human.
<!-- SECTION:NOTES:END -->
