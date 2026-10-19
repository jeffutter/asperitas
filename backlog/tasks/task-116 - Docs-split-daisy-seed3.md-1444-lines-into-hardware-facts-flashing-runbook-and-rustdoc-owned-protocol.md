---
id: TASK-116
title: >-
  Docs: split daisy-seed3.md (1,444 lines) into hardware facts, flashing
  runbook, and rustdoc-owned protocol
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 22:45'
labels:
  - review-2026-10
  - docs
  - planned
dependencies:
  - TASK-116.01
  - TASK-116.02
priority: medium
type: chore
ordinal: 218800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
daisy-seed3.md is 108 KB and perhaps 250 lines are hardware facts. The rest: console protocol v1 and BOOT/STATUS (208-341, which admits frame.rs is normative and the page 'a transcription', so they will drift), the rig record grammar (343-380, already pinned by tests), a ~680-line probe runbook (403-1083) with deep upstream source-line citations and a 90-line RTT/D-cache analysis for something 'not scheduled', host control wiring (1085), the excerpt format (1327-1434, which excerpt.rs:22+ already tables), capture budgets. It has 50 TASK- references and narration ('Before TASK-059 landed ... this page spent a week insisting', 122-130; 'An earlier revision claimed...', 148-150) and stale measurements (:119 gives main 88,581 bytes; main.bin is 105.1 K). A session told to read it before touching firmware pays a lot of context, and the volume is how the stale text in the probe ticket survived.

Change: daisy-seed3.md becomes hardware only (~200 lines); docs/reference/flashing.md is a terse DFU+probe runbook; console, rig, excerpt and host-control grammar move into rustdoc in frame.rs, console.rs, excerpt.rs, control.rs (already normative) with one short protocol index; investigations (RTT/D-cache, nRESET, reset-catch, .sram1_bss) go to docs/investigations/ or the tickets; history is left to git.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 daisy-seed3.md is hardware facts only; CLAUDE.md links the new files
- [ ] #2 Protocol grammar has one owner (rustdoc) and the page no longer transcribes it
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 55d0576

Approach: the page is mostly material that belongs elsewhere. Split into two sub-tickets (protocol to rustdoc; runbook and investigations to new files), which are independent. This ticket then finishes the page.

Note: docs/reference/daisy-seed3.md has uncommitted edits at planning time; rebase on whatever is committed before starting.

Steps after both children are Done:
1. Reduce docs/reference/daisy-seed3.md to hardware facts only (~200 lines): differences from earlier Seeds, strapped codec, SAI config, SDRAM/MPU/cache facts, libDaisy has no Seed3 support, stock bootloader facts that are hardware. Strip TASK- references, narration, stale measurements (e.g. main size). Replace moved sections with one-line links to flashing.md, the protocol index and docs/investigations.
2. Update CLAUDE.md hardware reference list to link flashing.md, the protocol index and investigations; fix links in docs/reference/*.md, docs/development.md and code comments that point at moved anchors (grep daisy-seed3.md).
3. Verify: grep -c TASK- on the page is 0, wc -l near 200, no remaining protocol grammar transcription, all links resolve.

Remaining: none beyond the children.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
planning marked this Dev Ready, but its dependencies are not all Done, so it was forced to Blocked. It should be re-checked once its dependencies are Done.
<!-- SECTION:NOTES:END -->
