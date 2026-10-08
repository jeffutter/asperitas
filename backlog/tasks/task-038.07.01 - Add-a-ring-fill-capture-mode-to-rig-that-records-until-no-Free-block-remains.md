---
id: TASK-038.07.01
title: Add a ring-fill capture mode to rig that records until no Free block remains
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 13:56'
updated_date: '2026-10-08 17:06'
labels:
  - planned
dependencies: []
parent_task_id: TASK-038.07
priority: medium
ordinal: 127800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rig's capture window is a build-time number of seconds, const-gated to fit inside the ring (rig.rs, `WINDOW_BLOCKS < capture::RING_BLOCKS`). TASK-038.07 needs a run that deliberately fills the ring, so the device's claim about maximum capturable duration (`CAPMAX seconds_max`) can be checked against the wall clock.

Add a build-time way to select 'capture until the ring is full'. Suggested: `ASP_RIG_CAPTURE_SECONDS=ring`, parsed by the existing const parser, so there is still one knob and junk still fails the build. In that mode the window gate does not apply and the capture ends when the producer finds no Free block. The producer already does the right thing there: it disarms, counts one overrun and never overwrites a block (`Producer::claim`). So the work is in the timeline (`run_capture`) and the judging (`judge_capture`):
- End the window on the producer's disarm instead of a deadline.
- Expect exactly `capture::RING_BLOCKS` delivered.
- Report the overrun that ended it as the expected terminator, not as a FAIL.
- Log the device's own elapsed capture time in ms (first armed callback to disarm), so the bench can set it beside `CAPMAX seconds_max` and the wall clock.

Bench findings from 2026-10-07 that apply: a full-ring dump is about 59 MB on the wire, about 81 s at the measured 731.7 kB/s (TASK-038.05 notes). The default 300 s build must stay byte-for-byte what it is, and its gates unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `ASP_RIG_CAPTURE_SECONDS=ring` (or an equivalent single build-time knob) builds a rig that captures until no Free block remains, and any value that is neither a whole number of seconds nor that word still fails the build
- [x] #2 In ring mode, the capture ends on the producer's disarm, the judge expects delivered == capture::RING_BLOCKS, reports the terminating overrun as expected rather than as a failure, and logs the measured capture duration in ms from the first armed callback to the disarm
- [x] #3 The default (300 s) build keeps its gates and its timeline unchanged, and a host-checkable test or const assert covers the parser's new arm
- [x] #4 scripts/gates.sh gains a push-tier build of the ring-mode variant, and `scripts/gates.sh push` passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 3ab48bf. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Also discharges the 'or until the ring reports full' clause of TASK-038.03 #10 and TASK-038.03.02 #9, which were closed on 2026-10-08 with that clause moved here.

Implemented in firmware/src/bin/rig.rs. ASP_RIG_CAPTURE_SECONDS now parses into a Window enum (Seconds(n) | RingFill) via parse_window; the exact word `ring` selects RingFill, digits select Seconds, anything else (Ring, rings, 30s, empty) still fails the build - all four checked by hand. Const asserts pin the parser arms (ring, 300, 0). RING_FILL derives WINDOW_BLOCKS = RING_BLOCKS, WINDOW_BYTES = RING_BYTES (CAPMAX total_bytes; unused_headroom 0), CAPTURE_SECONDS = ring_seconds_floor() (RIGCFG window_s = 349). The window-fits-ring gate is exempt in ring mode; a new const assert holds the backstop deadline past ring_duration_micros.

Timeline: one loop for both modes; in ring mode the deadline is a backstop (capacity floor + RING_FILL_BACKSTOP_SECONDS = 359 s) that only fires if audio stops, so the run still judges and dumps. The window normally ends on Producer::claim's overrun disarm. claim() stamps FILL_STARTED_MS (first successful claim = first armed callback) and FILL_ENDED_MS (the refused claim, stored before the ARMED release) from embassy_time ms; DWT cannot time 349.5 s (CYCCNT wraps every 8.9 s). judge_capture in ring mode: delivered == WINDOW_BLOCKS (== RING_BLOCKS) unchanged line; overrun gate becomes '== 1 (the full ring ending the window, expected)'; logs 'ring filled in N ms, first armed callback to disarm (CAPMAX seconds_max 349, us_max 349525333)', or an error FAIL line if either stamp is missing.

Default build: every ring branch is a const-false if, so it compiles out. Compared rust-objcopy -O binary images of HEAD and this change: same size (121.7K); 86 bytes differ, all of them panic-Location line fields in .rodata and the MOVW immediates that load line numbers, because code above them moved. No ring-mode string is present in the default image. Ring image is 122.2K, inside the 128 KB flash.

Gates: scripts/gates.sh gains push gate rig-ring-fill-build (ASP_RIG_CAPTURE_SECONDS=ring cargo build --release --features seed3 --bin rig). Priced with GATE_COSTS_BOOTSTRAP=1 scripts/gate-costs.sh --refresh, then the same without the flag (green); that rewrote cost figures in ci.yml, Makefile, elf-provenance.sh, check-elf-staleness.sh, doc-001 and docs/gate-costs.json. Clippy -D warnings clean for both default and ring builds. scripts/gates.sh push exits 0.

Pre-existing breakage fixed in passing: the push tier was red at HEAD (1b99dc9) because b46c36f (TASK-038.04.02) left three rustdoc links to private items in crates/asperitas-logging/src/usb.rs (CdcTx, CdcRx, inbound_reader) that fail cargo doc --all-features -D warnings. Turned them into plain code spans. README rig section gains the ring-mode flash line.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig gains a ring-fill capture mode, selected with ASP_RIG_CAPTURE_SECONDS=ring through the same compile-time parser (junk still fails the build). In that mode the window ends on the producer's overrun disarm after exactly capture::RING_BLOCKS, the judge expects delivered == RING_BLOCKS and reports the terminating overrun as expected (overrun == 1), and the device logs its own fill time in ms (first armed callback to disarm, embassy_time stamps taken in Producer::claim) beside CAPMAX seconds_max/us_max. A backstop deadline 10 s past capacity keeps a stalled run from hanging. The default 300 s build keeps its gates and timeline; its image differs only in panic line numbers. scripts/gates.sh has a new push gate, rig-ring-fill-build, with costs refreshed; the push tier passes, after fixing three pre-existing private-item rustdoc links in asperitas-logging/src/usb.rs that had turned it red.
<!-- SECTION:FINAL_SUMMARY:END -->
