---
id: TASK-038.04.04
title: >-
  Add the install state machine and examples/excerpt_stream.rs with pinned
  golden frames
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-08 16:07'
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
- [ ] #1 excerpt::install_frames(name, pcm, seq0, sink) emits the full EXCSTART, EXCDATA..., EXCEND frame sequence through a sink with no alloc and no clock (fixed level, caller-supplied seq and t_ms); examples/excerpt_stream.rs is a thin std wrapper that reads a WAV via parse_wav and writes those frames to stdout
- [ ] #2 Installer state machine (no I/O): feed one parsed record, get back an action - NeedSector(write 4096-byte sector at index), Verify, or Reject - holding exactly one 4096-byte sector buffer; header sector is invalidated at EXCSTART and written only after a successful verify
- [ ] #3 Strictness: EXCDATA i must equal the next expected index, so gaps, duplicates and reordering fail; odd or over-declared byte counts, bytes over slot capacity, EXCEND before all bytes, data without EXCSTART, and a new EXCSTART mid-install (aborts the old one) are all covered; a failed install can never reach the success verdict
- [ ] #4 Verdict: Installer::verdict(got_crc, got_len) yields Ok or Fail; exc_ok_body / exc_fail_body render 'EXCOK name= bytes= crc16=' and 'EXCFAIL got=<4hex> want=<4hex>' plus a short why= tag for length and protocol failures; bodies fit frame::MAX_BODY
- [ ] #5 Pinned golden frames for a small deterministic synthetic WAV (e.g. 1000 samples) checked in under crates/asperitas-logging/tests/golden/ and compared byte for byte; drift fails a test with a regenerate hint; plus a proptest that streams random PCM through install_frames, decodes with frame::Decoder in random chunkings, runs the Installer and asserts the reassembled sectors equal the source
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Depends on TASK-038.04.01 (excerpt module with constants, SlotHeader, parse_wav, EXCSTART/EXCDATA/EXCEND builders and parse_record). Extend crates/asperitas-logging/src/excerpt.rs (or a sibling install.rs re-exported from it if excerpt.rs gets long); stay ungated, no_std, no alloc.

1. install_frames<F: FnMut(&[u8])>(name, pcm:&[u8], seq0, t_ms, sink): build EXCSTART (bytes=pcm.len(), crc16=crc16_ccitt(pcm)), then EXCDATA chunks of the chunk size fixed in .01 with i counting from 0, then EXCEND; each body through frame::encode into a [u8;MAX_FRAME] stack buffer, one sink call per whole frame. Returns the next seq. Because the encoder is shared code, the example and the tests cannot disagree about the wire.
2. Installer: enum state {Idle, Receiving{name, declared, got, next_i, crc running, sector_fill}, Failed}. Sector accumulation: chunk size does not divide 4096, so bytes accumulate into one [u8;4096]; when full return Action::WriteSector{index, data}; the caller must call back before the next feed (borrow-enforced by returning a slice tied to &mut self). Final partial sector is padded with 0xFF and flushed at EXCEND. Running CRC is for cross-check only; the authoritative check is the device readback fed to verdict(). Sector index 0 is the header sector (from .01), PCM begins at sector 1; EXCSTART returns Action::InvalidateHeader so a half-installed slot reads as empty, and Action::WriteHeader is returned only by verdict(Ok). Failure is sticky until the next EXCSTART.
3. Records: exc_ok_body, exc_fail_body (reasons: crc, length, protocol); keep EXCFAIL got=/want= first so the AC format holds, with why= appended.
4. Example excerpt_stream: args <wav> <name> [--seq0 N]; parse_wav, install_frames to a BufWriter on stdout, errors to stderr with exit code 2; doc header explains the redirect to the CDC node and that correctness is decided by the device readback, not the pipe. Follow dump_reassemble's doc style.
5. Tests in tests/excerpt_install.rs: transition table per state; every failure in the strictness AC; chunking proptest; golden file tests/golden/excerpt_stream.bin generated with UPDATE_GOLDENS=1 (same convention as the audio goldens), asserted with a message pointing at the regenerate command. Keep the synthetic WAV deterministic (ramp), never random.

Verify: cargo test -p asperitas-logging (builds the example too); cargo clippy; confirm no firmware build impact. Hardware untouched; TASK-038.04.05 consumes Installer for the device side.

Risks: chunk size not dividing the sector means sector boundaries fall mid-chunk, which is where off-by-one bugs live - test with PCM lengths of 0, 1 sample, exactly 4096, 4096+2, and the slot maximum.
<!-- SECTION:PLAN:END -->
