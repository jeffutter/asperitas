---
id: TASK-116
title: >-
  Docs: split daisy-seed3.md (1,444 lines) into hardware facts, flashing
  runbook, and rustdoc-owned protocol
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-16 18:14'
labels:
  - review-2026-10
  - docs
dependencies:
  - TASK-114
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
