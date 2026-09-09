---
id: TASK-038.02.02
title: >-
  Define the AUDIO and AUDEND record grammar with compile-time-derived chunk
  geometry
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 15:51'
updated_date: '2026-09-09 15:54'
labels:
  - planned
dependencies:
  - TASK-038.02.01
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/tests/console_dump.rs
parent_task_id: TASK-038.02
priority: high
type: task
ordinal: 63500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With the payload codec in hand, this ticket fixes the on-wire grammar for audio blocks and makes its geometry impossible to drift from the code that produces it.

Two record bodies inside v1: `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<base64>` (constant 199-byte body, 129 raw bytes in 172 characters) and `AUDEND blk=<4hex> n=<2hex> bytes=<dec> crc16=<4hex>`, whose CRC covers the raw concatenated bytes. Short keys and fixed-width lowercase hex are forced by arithmetic, not taste: `MAX_BODY` bounds the whole body, so descriptive keys would cost 18 of 129 raw bytes — see the budget table in TASK-038.02's Implementation Notes, which also records that any header between 25 and 28 bytes yields the same 129.

`CHUNK_RAW` must be *derived* at compile time from the same byte templates the writer emits, and pinned by `const` assertions, so that AC #6's published efficiency numbers cannot quietly disagree with the encoder.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `dump::audio_body` and `dump::audend_body` build complete record bodies into a caller-provided `[u8; MAX_BODY]`, emit constant-length headers (`AUDIO blk=` + 4 lowercase hex digits + ` n=` + 2 + ` c=` + 2 + ` d=` = 27 bytes), and refuse `chunks > MAX_CHUNKS_PER_BLOCK` (256), an empty chunk, or a chunk longer than `CHUNK_RAW` with a distinct error rather than truncating.
- [ ] #2 `CHUNK_RAW`, the header length, and the full frame length are derived by const evaluation from the byte templates the writer actually uses, and `const _: () = assert!(...)` pins `CHUNK_RAW == 129`, full body `== 199`, and full frame `== 227`.
- [ ] #3 One composite entry point per record kind composes body-building with `frame::encode` and returns `Encoded`, so no caller re-derives the buffer dance; `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, the record prefix, and the CRC polynomial are untouched.
- [ ] #4 Golden rows pin complete encoded frames — a full 129-byte chunk, a short final chunk with padding, and an `AUDEND` — each checked against an independently computed CRC literal in the style of `tests/console_frame.rs:457-485`, never against the encoder that produced them.
- [ ] #5 An arithmetic test derives useful-bytes-per-wire-byte, records/s, and wire kB/s for mono 16-bit capture at 96,000 B/s from the published constants and asserts the documented values (129/227 = 0.568, 745 records/s, ~169 kB/s), plus the mono 32-bit doubling; changing any geometry constant without updating the claim fails the test.
- [ ] #6 The module doc carries the grammar table verbatim (field names, widths, alphabet, what the block CRC covers, why keys are abbreviated) so TASK-038.06 has one source to copy. `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test -p asperitas-logging`, and the firmware release build pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Depends on TASK-038.02.01 (the codec). Geometry decisions are settled — do not re-litigate them; the derivation and the rejected alternatives are in TASK-038.02's Implementation Notes.

## Wire grammar to implement (constant-length header = 27 bytes)
```
AUDIO blk=<4 hex> n=<2 hex> c=<2 hex> d=<base64>      full chunk: 27 + 172 = 199 body, 227-byte frame, 129 raw bytes
AUDEND blk=<4 hex> n=<2 hex> bytes=<dec> crc16=<4 hex>  ≤ 46 body; crc16 covers the RAW concatenated block bytes
```
Lowercase hex only (`frame.rs:806` already pins lowercase-only parsing). `n` is the chunk count for the block, `c` the zero-based index, `blk` wraps at 65,536 blocks (a full 32 MiB ring at 8 KiB blocks is 4,096 — 16× headroom; cross-run ordering is the frame `seq`'s job). Recommend 64 chunks per block (8,256 raw ≈ 86 ms of mono 16-bit) to TASK-038.03 in a doc comment.

## Derive, don't hardcode
```rust
const AUDIO_PREFIX: &[u8] = b"AUDIO blk=";   // 10
const SEP_N: &[u8] = b" n=";                 // 3
const SEP_C: &[u8] = b" c=";                 // 3
const SEP_D: &[u8] = b" d=";                 // 3
const BLK_HEX: usize = 4;
const COUNT_HEX: usize = 2;
const HDR_LEN: usize = AUDIO_PREFIX.len() + BLK_HEX + SEP_N.len() + COUNT_HEX
                     + SEP_C.len() + COUNT_HEX + SEP_D.len();            // 27
const FULL_B64_CHARS: usize = (MAX_BODY - HDR_LEN) / 4 * 4;              // 172
pub const CHUNK_RAW: usize = FULL_B64_CHARS / 4 * 3;                     // 129
pub const MAX_CHUNKS_PER_BLOCK: usize = 1 << (COUNT_HEX * 4);            // 256
const _: () = assert!(CHUNK_RAW == 129);
const _: () = assert!(HDR_LEN + FULL_B64_CHARS == MAX_BODY - 1);         // 199
const _: () = assert!(PREFIX_LEN + HDR_LEN + FULL_B64_CHARS + TRAILER_LEN == 227);
```
The writer must emit from those same `&[u8]` templates, so a template edit moves `CHUNK_RAW` with it and the assertions catch the consequence. That is what makes AC #6 real rather than decorative.

## API
```rust
pub enum BodyError { TooManyChunks { chunks: usize }, EmptyChunk, ChunkTooLong { len: usize } }
pub fn audio_body(block_index: u32, chunks: u16, chunk_index: u16, raw: &[u8], out: &mut [u8; MAX_BODY]) -> Result<usize, BodyError>;
pub fn audend_body(block_index: u32, chunks: u16, total_bytes: u32, crc: u16, out: &mut [u8; MAX_BODY]) -> usize;
pub fn audio_record(level: Level, seq: u32, now_ms: u32, block_index: u32, chunks: u16, chunk_index: u16,
                    raw: &[u8], body: &mut [u8; MAX_BODY], frame: &mut [u8; MAX_FRAME]) -> Result<Encoded, BodyError>;
pub fn audend_record(level: Level, seq: u32, now_ms: u32, block_index: u32, chunks: u16, total_bytes: u32, crc: u16,
                     body: &mut [u8; MAX_BODY], frame: &mut [u8; MAX_FRAME]) -> Encoded;
```
The `_record` functions compose `frame::encode` so callers never re-derive the buffer dance; they take both buffers explicitly rather than hiding a static, and TASK-038.02.04's pipe path supplies `frame` from the existing `RECORD_BUFS`, so firmware still pays no stack cost. Use `Level::Info` like BOOT/STATUS and say why in the doc comment (data records are not debug chatter; the decoder accepts only `IWEDT` anyway). Refuse rather than truncate: `chunks > MAX_CHUNKS_PER_BLOCK`, `chunk_index >= chunks`, empty `raw`, and `raw.len() > CHUNK_RAW` each return their own variant.

## Hex/decimal writers
Widen `frame.rs`'s private `write_hex` (:252) and `write_decimal` (:263) to `pub(crate)` and reuse them — one owner of digit formatting, both already `debug_assert!` their widths, and nothing outside the crate can see the change. Do not copy them into `dump.rs`, and do not reach for `core::fmt`/`TruncWriter`: the record bodies here are fixed-shape and byte-level writers keep the geometry arithmetic honest.

## Tests (append to tests/console_dump.rs)
1. Golden rows with **independent** CRC literals, following `tests/console_frame.rs:457-485` — build the expected wire bytes as a literal, then `assert_eq!(crc16_ccitt(&wire[1..body_end]), 0x....)` against a hand-pinned value, and separately assert `encode(...)` reproduces the literal. Rows needed: full 129-byte chunk, short final chunk exercising `=` padding (pick a length ≡ 1 mod 3 and one ≡ 2 mod 3), an `AUDEND`, and a two-block interleaved pair proving `blk` distinguishes them.
2. `header_is_constant_length_for_every_valid_field_combination` — proptest over `block_index`, `chunks`, `chunk_index`, and payload lengths 1..=CHUNK_RAW: every produced body has length `HDR_LEN + encoded_len(len)`, and a full chunk yields exactly 227 wire bytes with `Encoded.truncated == false`.
3. `refuses_instead_of_truncating` — the four refusal variants, each asserted to return its specific error and write nothing.
4. `published_efficiency_matches_the_encoder` (AC #6) — integer arithmetic only: `(CHUNK_RAW as u32 * 1000) / 227 == 568`; records/s for 96,000 B/s capture as `(96_000 + CHUNK_RAW - 1) / CHUNK_RAW == 745`; wire bytes/s `745 * 227 == 169_115`; and the mono-32-bit doubling to 338 kB/s. Each assertion message states the claim in prose so a future failure tells the reader which document to update. Add a comment recording that the ticket's original 150-raw-byte premise was impossible and where the corrected table lives.
5. Round-trip through the real decoder: encode a handful of AUDIO/AUDEND records, feed the concatenated frames to `frame::Decoder`, and assert every record comes back with its body intact and `stats().bad_frames == 0` — this catches anything `sanitize_byte` would mangle before the assembler ever sees it.

## Verification
Same four commands as TASK-038.02.01, plus confirm the pinned golden CRCs survive a clean re-run after `cargo clean -p asperitas-logging`. No `log-usb` code, no changes under `firmware/`.
<!-- SECTION:PLAN:END -->
