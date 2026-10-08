---
id: TASK-038.04.01
title: >-
  Add the excerpt module: slot layout constants, slot header, WAV parser and
  EXCSTART/EXCDATA/EXCEND grammar to asperitas-logging
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-08 16:06'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-038.04
priority: high
ordinal: 131800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: new host-testable no_std module in crates/asperitas-logging (no feature gate, like frame/dump). Owns the layout constants (area at 0x100000, stride 512 KiB, fourteen slots, top 4 KiB sector reserved because MAX_ADDRESS=0x7FFFFF, nothing below 0x40000), slot address arithmetic, slot header encode/validate, a WAV header parser that accepts only mono 48 kHz 16-bit PCM and rejects everything else, and the record grammar EXCSTART name=<tag> bytes=<n> crc16=<c>, EXCDATA i=<j> b64=<payload>, EXCEND, reusing the dump base64 codec and frame CRC. Grammar must stay clear of the '<>' bytes reserved host-to-device. Acceptance: host tests for slot arithmetic (every slot sector-aligned, last slot ends below the reserved sector), header validation, WAV rejection from synthetic inputs, grammar round trip. Covers parent AC #1 (constants half) and AC #7 (arithmetic, header, WAV).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Constants in one module: EXCERPT_AREA_START=0x100000, SLOT_STRIDE=0x80000, SLOT_COUNT=14, SECTOR=4096, reserved top sector, with const asserts that last slot end <= 0x7FFFFF minus the reserved sector and that the area start >= 0x40000
- [x] #2 slot_base(n) rejects n >= SLOT_COUNT; every slot base is sector-aligned; host tests cover all 14 slots
- [x] #3 Slot header encode/validate: magic, version, byte length, CRC-16; rejects bad magic, length over slot capacity or odd, and CRC mismatch; round-trip tested
- [x] #4 parse_wav accepts only PCM mono 48000 Hz 16-bit and returns the data span; rejects non-RIFF/WAVE, wrong format tag, channels, rate, bit depth, truncated data, odd data length, extra chunks handled by skipping; tested on synthetic inputs and on audio/instruments/*.wav
- [x] #5 EXCSTART/EXCDATA/EXCEND body builders and parsers, reusing dump base64 and frame CRC, bodies fit frame::MAX_BODY, name tag charset excludes '<' '>' '~' and spaces; round-trip and rejection tests; no hardware or feature gate
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by fa49366. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented crates/asperitas-logging/src/excerpt.rs (ungated, no_std, no alloc) and tests/excerpt.rs (23 tests incl. 3 proptests). Nothing existed before this ticket.

Decisions that differ from or extend the plan - TASK-038.04.04/.05 should read these:
- Layout: 14 strides of 512 KiB from 0x100000 end exactly at 0x800000, one sector past the driver bound. Every slot therefore gives up its top (guard) sector: SLOT_SPAN = 0x7F000, header sector at slot_base, PCM at slot_base + 4096, PCM_CAPACITY = 516,096 bytes for every slot. Slot 13's guard sector is exactly the reserved sector 0x7FF000 (const-asserted). Constant names: EXCERPT_AREA_START, SLOT_STRIDE, SLOT_COUNT (u8), SECTOR, MAX_ADDRESS, RESERVED_SECTOR_START, BOOTLOADER_END, PCM_OFFSET, PCM_CAPACITY, EXCERPT_AREA_END.
- EXCSTART carries a slot field: 'EXCSTART slot=<2 hex> name=<tag> bytes=<6 dec> crc16=<4 hex>'. Parent AC #2's grammar has no way to say which slot to write; deriving it from the name would require a header scan per install.
- Name tag is up to 16 chars of [a-z0-9_] (plan said 8; 8 is too tight to name the corpus clips). Header is 30 bytes: magic ASPX, u16 version 1, 16-byte NUL-padded name, u32 pcm_bytes, u16 PCM CRC, u16 header self-CRC over bytes 0..28. 'CRC mismatch' in AC #3 is the header self-CRC; the PCM CRC is stored for replay to compare against.
- EXC_CHUNK_RAW = 128 (not 129/135): 32 chunks per sector exactly, chunks never straddle a sector and are whole samples; max 4,032 chunks, index is 4 hex. Max bodies: EXCSTART 62, EXCDATA 191 (MAX_BODY 200).
- parse_record returns ExcRecord::{Start, Data{index, chunk: Chunk (decoded, inline [u8;128])}, End}; range checks (slot, length even/non-zero/within capacity, index, chunk 1..=128) happen in both builders and parser via one shared check_pcm_len. Sequencing is left to the installer.
- dump::BodyReader and dump::put/put_hex/put_decimal became pub(crate) so the grammar reuses dump's reader and writers instead of copying them.
- parse_wav walks chunks (skips LIST/fact/unknown, honours odd-size pad bytes), bounds the walk by the RIFF size, requires fmt before data, tag 1, mono, 48000, 16-bit, block_align 2, byte_rate 96000; rejects empty/odd data. All eight audio/instruments clips parse and fit a slot.

Verified: cargo test -p asperitas-logging (166 passed), clippy -D warnings clean, cargo doc clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the excerpt module to asperitas-logging: QSPI slot layout constants with compile-time bounds (14 slots x 512 KiB from 0x100000, each slot's top sector a guard so the last lands on the reserved 0x7FF000 sector; 516,096 PCM bytes per slot), slot_base/slot_pcm_base, a 30-byte self-checking SlotHeader, a strict chunk-walking parse_wav for mono 48 kHz 16-bit PCM, and the EXCSTART/EXCDATA/EXCEND body builders and parse_record reusing dump's base64 and body reader. EXCSTART gained a slot field and tags allow 16 chars; see notes. Host tests in tests/excerpt.rs cover slot arithmetic for all slots, every single-bit header corruption, a WAV rejection table plus the real corpus, and grammar round trips/rejections via proptest.
<!-- SECTION:FINAL_SUMMARY:END -->
