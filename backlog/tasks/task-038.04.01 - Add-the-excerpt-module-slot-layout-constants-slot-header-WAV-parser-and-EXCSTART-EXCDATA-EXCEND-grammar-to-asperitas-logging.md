---
id: TASK-038.04.01
title: >-
  Add the excerpt module: slot layout constants, slot header, WAV parser and
  EXCSTART/EXCDATA/EXCEND grammar to asperitas-logging
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-08 15:54'
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
- [ ] #1 Constants in one module: EXCERPT_AREA_START=0x100000, SLOT_STRIDE=0x80000, SLOT_COUNT=14, SECTOR=4096, reserved top sector, with const asserts that last slot end <= 0x7FFFFF minus the reserved sector and that the area start >= 0x40000
- [ ] #2 slot_base(n) rejects n >= SLOT_COUNT; every slot base is sector-aligned; host tests cover all 14 slots
- [ ] #3 Slot header encode/validate: magic, version, byte length, CRC-16; rejects bad magic, length over slot capacity or odd, and CRC mismatch; round-trip tested
- [ ] #4 parse_wav accepts only PCM mono 48000 Hz 16-bit and returns the data span; rejects non-RIFF/WAVE, wrong format tag, channels, rate, bit depth, truncated data, odd data length, extra chunks handled by skipping; tested on synthetic inputs and on audio/instruments/*.wav
- [ ] #5 EXCSTART/EXCDATA/EXCEND body builders and parsers, reusing dump base64 and frame CRC, bodies fit frame::MAX_BODY, name tag charset excludes '<' '>' '~' and spaces; round-trip and rejection tests; no hardware or feature gate
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

New file crates/asperitas-logging/src/excerpt.rs, registered as 'pub mod excerpt;' beside dump/capture in lib.rs (ungated, no_std, no alloc). Model on dump.rs: const-derived geometry with compile-time asserts, body builders returning Result with a BodyError enum, parsers returning typed errors.

1. Constants: AREA_START 0x100000, SLOT_STRIDE 0x80000, SLOT_COUNT 14, SECTOR_SIZE 4096, MAX_ADDRESS mirror 0x7FFFFF (driver bound is address+len <= MAX_ADDRESS, so top sector stays unused), SLOT_CAPACITY = stride minus one sector reserved for the header (put header in the slot's first sector, PCM from slot_base+SECTOR_SIZE so PCM stays sector aligned). Const asserts: last slot end < 0x7FF000, AREA_START >= 0x40000, stride multiple of SECTOR_SIZE.
2. slot_base(n)->Option<u32>, slot_pcm_base(n), max PCM bytes. Largest corpus clip is ~288 kB so capacity 508 KiB is ample.
3. SlotHeader {magic 'ASPX', version, pcm_bytes u32, crc16 u16, name 8-byte tag}; encode into [u8;N] little-endian, validate. Use frame::crc16_ccitt.
4. parse_wav(&[u8]) -> Result<WavData{pcm:&[u8]}, WavError>: walk RIFF chunks (skip unknown chunks incl. LIST, pad odd sizes), require fmt tag 1, ch 1, rate 48000, bits 16, block align 2, and data length even and within the buffer. Real files have the canonical 44-byte header (checked: mandolin_chord.wav) but do not assume it.
5. Grammar per parent AC #2: 'EXCSTART name=<tag> bytes=<n> crc16=<4hex>', 'EXCDATA i=<4hex> b64=<payload>', 'EXCEND'. Chunk raw size = dump::CHUNK_RAW or smaller so the body fits MAX_BODY=200; reuse dump::encode/decode. Name tag [a-z0-9_] up to 8 chars (mirrors header), which also excludes '<' '>' '~' and space. Provide exc_start_body, exc_data_body, exc_end_body and a single parse_record(&[u8]) -> Result<ExcRecord,_> the install state machine (TASK-038.04.04) will consume. Keep the interface small; state machine is not in this ticket.
6. Tests: unit tests in-module plus tests/excerpt.rs: slot arithmetic exhaustive, header validation mutation per field, WAV rejection table built from a synthetic header builder, parse of every audio/instruments file (path relative to CARGO_MANIFEST_DIR), grammar round trip with proptest (dev-dep present), and rejection of bodies containing '<' '>' or bad hex.

Verify: cargo test -p asperitas-logging; cargo clippy workspace lints; firmware build unaffected (module is ungated and dependency-free). Do not pin goldens here; those belong to TASK-038.04.04.
<!-- SECTION:PLAN:END -->
