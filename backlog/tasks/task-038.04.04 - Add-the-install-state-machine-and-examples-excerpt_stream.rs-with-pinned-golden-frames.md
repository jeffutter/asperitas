---
id: TASK-038.04.04
title: >-
  Add the install state machine and examples/excerpt_stream.rs with pinned
  golden frames
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-08 16:23'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.01
parent_task_id: TASK-038.04
priority: high
ordinal: 134800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: in the excerpt module, a pure state machine that consumes decoded EXCSTART/EXCDATA/EXCEND records and yields whole 4 KiB sectors for sector-aligned writing, one sector buffered at a time, then a verdict (length and CRC-16 compare producing EXCOK or EXCFAIL got=<crc> want=<crc>). Truncation, out-of-order i, duplicate, oversize, and corrupted uploads must never yield success. Plus crates/asperitas-logging/examples/excerpt_stream.rs: reads a WAV path and slot, writes framed install records to stdout (no serial dependency). Acceptance: host tests for every state-machine transition and failure; excerpt_stream output checked against pinned golden frames. Covers parent AC #2 (protocol half), AC #3 (verdict logic) and AC #7 (state machine, goldens).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 excerpt::install_frames(name, pcm, seq0, sink) emits the full EXCSTART, EXCDATA..., EXCEND frame sequence through a sink with no alloc and no clock (fixed level, caller-supplied seq and t_ms); examples/excerpt_stream.rs is a thin std wrapper that reads a WAV via parse_wav and writes those frames to stdout
- [x] #2 Installer state machine (no I/O): feed one parsed record, get back an action - NeedSector(write 4096-byte sector at index), Verify, or Reject - holding exactly one 4096-byte sector buffer; header sector is invalidated at EXCSTART and written only after a successful verify
- [x] #3 Strictness: EXCDATA i must equal the next expected index, so gaps, duplicates and reordering fail; odd or over-declared byte counts, bytes over slot capacity, EXCEND before all bytes, data without EXCSTART, and a new EXCSTART mid-install (aborts the old one) are all covered; a failed install can never reach the success verdict
- [x] #4 Verdict: Installer::verdict(got_crc, got_len) yields Ok or Fail; exc_ok_body / exc_fail_body render 'EXCOK name= bytes= crc16=' and 'EXCFAIL got=<4hex> want=<4hex>' plus a short why= tag for length and protocol failures; bodies fit frame::MAX_BODY
- [x] #5 Pinned golden frames for a small deterministic synthetic WAV (e.g. 1000 samples) checked in under crates/asperitas-logging/tests/golden/ and compared byte for byte; drift fails a test with a regenerate hint; plus a proptest that streams random PCM through install_frames, decodes with frame::Decoder in random chunkings, runs the Installer and asserts the reassembled sectors equal the source
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 8441239. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in crates/asperitas-logging/src/excerpt/install.rs (re-exported from excerpt): install_frames, Installer (feed/feed_body/verdict/is_open), Action, Verdict, Failure, Why, exc_ok_body, exc_fail_body. Nothing pre-existed beyond the .01 grammar.

Deviations from the plan, all because .01 fixed EXC_CHUNK_RAW = 128, which divides SECTOR: a chunk never straddles sectors, so one feed yields at most one WriteSector, and the final partial sector (padded 0xFF) is flushed by the EXCDATA that completes the declared length rather than at EXCEND. EXCEND therefore only ever answers Verify. install_frames takes slot too (EXCSTART needs it) and t_ms from the caller; level fixed at Info (INSTALL_LEVEL). Example args are <wav> <slot> <name> [--seq0 N].

Header handling: EXCSTART -> Action::InvalidateHeader{slot,address,aborted}; only Verdict::Ok carries the SlotHeader and its encoded bytes, so the header write is the last flash op. Running CRC is checked at EXCEND (why=stream) as a cross-check; verdict() is authoritative (length checked before CRC).

Failure is sticky: one Reject per install, then Ignored until the next EXCSTART (EXCEND in Failed returns to Idle). Data in Idle -> nostart then sticky. Odd/over-capacity EXCSTART lengths are refused by parse_record, surfaced by feed_body as why=malformed; odd chunk lengths fail as why=short. EXCFAIL keeps got=/want= first; plain readback CRC mismatch has no why= tag, every other failure appends one (length, stream, nostart, order, short, overrun, early, malformed, state). Bodies const-asserted at 51 (EXCOK) and 40 (EXCFAIL) <= MAX_BODY.

Goldens: tests/golden/excerpt_ramp.wav (1000-sample ramp) and tests/golden/excerpt_stream.bin (slot 3, ramp1000, seq0 0, t_ms 0), regenerated with UPDATE_GOLDENS=1 cargo test -p asperitas-logging --test excerpt_install; drift panics with that hint. Verified the example itself: cargo run --example excerpt_stream -- tests/golden/excerpt_ramp.wav 3 ramp1000 | cmp - tests/golden/excerpt_stream.bin is identical. Proptests: random PCM x random decoder chunkings round-trip to Ok; dropping any single frame never verifies.

Verification: cargo test -p asperitas-logging (207 pass, 29 in excerpt_install), scripts/gates.sh commit (15 gates incl. firmware clippy) green. Firmware does not call any of this yet; TASK-038.04.05 wires Installer to flash.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the excerpt install protocol in crates/asperitas-logging/src/excerpt/install.rs: install_frames (host stream: EXCSTART, EXCDATA..., EXCEND, no alloc, caller seq/t_ms), the I/O-free Installer state machine (one 4 KiB sector buffer, InvalidateHeader at EXCSTART, WriteSector per full or final sector, Verify at EXCEND, header only via Verdict::Ok), strict failure handling (order/short/overrun/early/nostart/malformed/stream/state/length/crc), and EXCOK/EXCFAIL body renderers. Added examples/excerpt_stream.rs (WAV + slot + name -> frames on stdout) and tests/excerpt_install.rs with transition and failure tests, boundary lengths (2 B to full capacity), pinned goldens under tests/golden/, and proptests over random PCM and decoder chunkings.
<!-- SECTION:FINAL_SUMMARY:END -->
