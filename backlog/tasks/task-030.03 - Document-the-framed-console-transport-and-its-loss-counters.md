---
id: TASK-030.03
title: Document the framed console transport and its loss counters
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 03:25'
updated_date: '2026-09-09 09:16'
labels:
  - planned
dependencies:
  - TASK-030.02
documentation:
  - docs/reference/daisy-seed3.md
  - README.md
parent_task_id: TASK-030
priority: medium
type: docs
ordinal: 50500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: TASK-030, acceptance criterion #7. Write console protocol v1 down where someone looking for a debug channel will find it: the record grammar and CRC parameters in docs/reference/daisy-seed3.md's 'Debugging without a probe' section (currently lines 146-157), and an honest update to README.md's screen-based instructions (roughly lines 163-200) so they describe what a terminal now shows and how to decode a saved capture. Transcribe field names and widths from the shipped frame.rs and the emission site, not from this ticket. See the plan for the required content list.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/reference/daisy-seed3.md "Debugging without a probe" section states the v1 record grammar field by field with a real example line, and gives the CRC parameters (algorithm, poly, init, reflection, xorout, covered byte range, 123456789 -> 0x29b1 check vector) explicitly enough to reimplement from the document alone.
- [ ] #2 It documents why CRLF is a trustworthy delimiter, the reader resynchronisation rule, and what guarantee that rule buys.
- [ ] #3 It lists the reserved BOOT and STATUS body prefixes with every field, explains each loss counter, and says why sequence numbers alone cannot distinguish a reboot from a loss.
- [ ] #4 It records that the leading byte is reserved device-to-host and a different one for host-initiated commands, pointing at TASK-032, and states the measured ~8.8% baseline this replaces along with the caveat that its raw capture no longer exists.
- [ ] #5 README.md debugging instructions describe what a plain terminal now shows, keep the panictest countdown and PANIC procedure meaningful, and give the console_decode command for checking a saved capture plus a pointer to TASK-031.
- [ ] #6 Every field name, width and counter in both documents was checked against the shipped frame.rs and the emission site rather than transcribed from this ticket, and cargo fmt --all --check passes.
- [ ] #7 It records the USB short-packet rule: that the device terminates every bulk transaction with a short packet or zero-length packet, why an exactly-64-byte final packet would otherwise sit unseen in the host's driver buffer, and that a reader may legitimately observe zero-length reads.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan (mechanical; content derives from TASK-030 §3 and the shipped code)

Rewrite documentation so the transport's guarantees and its loss counters are written down where someone
looking for debug channels will find them. Two files.

**1. `docs/reference/daisy-seed3.md`**, section `### Debugging without a probe` (currently lines 146-157,
between the DFU notes and `### Debug probe, when one is available`). Today it says nothing about the log
format and nothing about loss. Add, after the existing prose:

- The v1 record grammar verbatim from TASK-030 §3, field by field, with one real example line.
- The CRC parameters spelled out (CRC-16/CCITT-FALSE, poly `0x1021`, init `0xFFFF`, non-reflected,
  `xorout=0`, check vector `123456789` ⇒ `0x29b1`) and the exact byte range covered.
- Why CRLF is a trustworthy delimiter (control bytes in bodies are replaced by `_`).
- The reader's resynchronisation rule, and the claim it buys: corruption costs one record, not the capture.
- The reserved `BOOT` and `STATUS` body prefixes with their fields, and what each counter means —
  including the reason seq alone is not enough (a seq gap cannot distinguish reboot from loss, which is why
  `BOOT` exists).
- That `~` is device→host and a different leading byte is reserved for host→device commands, pointing at
  TASK-032.
- One honest sentence on the old defect and the measured baseline it replaces: ~8.8% of lines truncated in
  the 2026-08-08 capture, counted by hand (`backlog/tasks/task-018.04 …:83`), with the note that the raw
  capture no longer exists so the figure is a baseline rather than a reproducible measurement.

**2. `README.md`**, the "Debugging Without a Probe" section (roughly lines 163-200). The `screen`
instructions currently imply unframed text. Say what a terminal now shows — each line prefixed
`~<level> <seq> <ms> ` and suffixed `*<crc>` — confirm the messages remain readable by eye, keep panictest's
countdown/`PANIC:` procedure intact, and add the decode command for checking a saved capture:
`cargo run -p asperitas-logging --example console_decode -- capture.raw` (from TASK-030.01), with a pointer
forward to the rig runner in TASK-031.

Cross-check every field name, width and counter against `crates/asperitas-logging/src/frame.rs` and the
emission site in `usb.rs` after TASK-030.02 lands — do not transcribe this ticket's text from memory.
Prose follows the project's usual style: state the trap and the reason, not a tutorial.
<!-- SECTION:PLAN:END -->
