---
id: TASK-038.04.06
title: >-
  Document excerpt storage layout, install and replay costs with predicted and
  observed columns
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-08 15:54'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03
parent_task_id: TASK-038.04
priority: high
ordinal: 136800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: docs/reference/daisy-seed3.md mirror of the layout constants (AC #1 doc half), the 96,000 B/s budget, install wall-clock including erases, replay margin, the rule that the blocking flash API is forbidden in this binary, and the DAC-path CRC posture. Observed columns stay marked pending until TASK-038.09 measures them on the bench. Also correct TASK-019.03's note that QSPI carries the firmware image via XIP (not true of this Rust stack, which links to internal flash). Coordinate wording with TASK-038.06. Covers parent AC #1 and AC #6.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/reference/daisy-seed3.md gains one section 'Instrument excerpts in QSPI' with a layout table whose values are copied from the excerpt module constants (area start 0x100000, stride 0x80000, 14 slots, header sector, reserved top sector, bootloader region below 0x40000 untouched) and a note naming the module as the owner; a host test or doc-consistency check fails if the table and the constants drift
- [ ] #2 A cost table with predicted and observed columns: 96,000 B/s mono 16-bit, bytes and sectors per corpus clip, install wall-clock (47 sector erases x 300 ms datasheet max for a 190 kB clip, with the per-page program term), replay staging margin, wire bytes for the install stream; every observed cell reads 'pending TASK-038.09' and every predicted cell states its derivation
- [ ] #3 States the rules: only the async flash API may be used in rig (blocking wait_for_write is an unbounded busy loop; async timeouts panic), writes are whole sector-aligned 4096-byte sectors, top sector reserved because the driver asserts against 0x7FFFFF
- [ ] #4 States the DAC-path CRC posture: the replay CRC equals the host file CRC and proves only what was handed to the DAC encoder, nothing about the codec or cable, matching TASK-035 AC #4; documents the EXCSTART/EXCDATA/EXCEND/EXCOK/EXCFAIL/EXCPLAY grammar with one worked line each, each pointing at the host test that pins it
- [ ] #5 TASK-019.03's 'QSPI also carries the firmware image via XIP' note is corrected with a dated correction (this stack links at 0x08000000 and has no execute-in-place path), unless TASK-038.06 already did so; nothing here duplicates the README workflow or SDRAM budget that TASK-038.06 owns
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Depends on TASK-038.04.03 (and so on every other sibling) so that every name, constant and record shape documented already exists; write from the code, not from this plan. Docs only, no firmware change.

Overlap with TASK-038.06 (README workflow, SDRAM and QSPI budget tables, record grammars, stale-statement fixes): this ticket owns only the excerpt-specific content, as one self-contained section in docs/reference/daisy-seed3.md that 038.06 links to from its README section and budget tables rather than restating. Before editing, read the current daisy-seed3.md and 038.06's state: if it landed first, extend its QSPI table instead of adding a duplicate; if not, put the section after 'External SDRAM: address, MPU and caches (measured)' (currently ~line 1054) so it sits beside the other memory material, leaving room for 038.06 to link. The existing doc already says at line 79 that the application is not placed in QSPI - reuse that statement.

Steps:
1. Read excerpt.rs constants, Installer, replay core, rig.rs install and replay code; copy figures from them.
2. Write the layout table, rules, cost table, posture paragraph and grammar examples. Mark each figure measured / derived / estimated. QSPI clock is an estimate (kernel clock unset, ~60 MHz CLK at the reset hclk3 selection, per the driver notes); say so.
3. Predicted install time: sectors = ceil(bytes/4096) + 1 header; erase datasheet max 300 ms, typical far lower (use datasheet typical if the IS25LP064A sheet is available in-repo, else cite max as an upper bound and label it as such); program 0.8 ms datasheet max per 256 B page x 16 pages per sector; USB transfer time at 96,000 B x 4/3 base64 x frame overhead (28/129 per chunk) over full-speed bulk - label as estimate, USB ceiling unmeasured.
4. Doc-drift guard: a small test in crates/asperitas-logging/tests (e.g. excerpt_docs.rs) that reads the doc via include_str! and asserts the hex constants and slot count appear in the table text; cheap, and honours AC 1 'mirrored'.
5. Fix the XIP sentence in backlog/tasks/task-019.03* by appending a dated correction line (do not rewrite history); skip if 038.06 already did.
6. Observed columns stay 'pending TASK-038.09'; do not invent readings.

Verify: cargo test -p asperitas-logging; render check by reading the Markdown tables; grep the doc for em dashes (project rule: plain dash only) and for any figure without a source label. Hardware untouched.
<!-- SECTION:PLAN:END -->
