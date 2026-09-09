---
id: TASK-030.05
title: Update panictest.rs expected-output table for the framed PANIC record
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 09:55'
labels: []
dependencies:
  - TASK-030.02
parent_task_id: TASK-030
priority: medium
type: task
ordinal: 54500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-030.02 wrapped the panic message in a self-verifying frame (TASK-030 §3 grammar), so the last line a developer sees on a raw terminal now carries the ~E prefix, seq, t_ms and *crc trailer. panictest.rs is the binary whose whole purpose is checking that line against a known-good expectation, and its header still describes the unframed form — the next person to run it will read '~E 0000000c 0000001512 PANIC: ...' as a defect.

Blocked from doing this inside TASK-030.02 by that ticket's AC #8, which forbids any change under firmware/ (verified with git diff --stat firmware/). Doc-comment-only edit; no code or call site changes. The board confirmation that the framed PANIC line arrives and decodes is TASK-030.04 / TASK-033, not this ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The 'Reading the output' table in firmware/src/bin/panictest.rs shows the panicked stage's serial line as a console v1 frame (~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C *<crc> CRLF) rather than the bare PANIC: text, and names usb::emit_panic_record as what pushes it.
- [ ] #2 cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings and cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release all pass; only doc comments in the binary change, so git diff of non-comment lines under firmware/ stays empty.
<!-- AC:END -->
