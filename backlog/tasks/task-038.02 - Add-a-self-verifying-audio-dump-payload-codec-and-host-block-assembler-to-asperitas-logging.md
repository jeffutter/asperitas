---
id: TASK-038.02
title: >-
  Add a self-verifying audio dump payload codec and host block assembler to
  asperitas-logging
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:33'
labels: []
dependencies: []
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/examples/dump_reassemble.rs
  - crates/asperitas-logging/tests/console_dump.rs
parent_task_id: TASK-038
priority: high
type: task
ordinal: 56500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Recorded audio has to leave the device through the console transport TASK-030 built, and that transport is printable-ASCII text with 200-byte bodies: `sanitize_byte` replaces every byte below 0x20, so raw sample bytes cannot travel. The frame grammar itself must not change — `MAX_BODY = 200`, `MAX_FRAME = 228` are compile-time asserted and shared by the encoder, the decoder, `PANIC_FRAME`, and `RecordBufs`, and widening them is a v2 wire change under TASK-030's own versioning rule. So this ticket adds a payload layer inside v1, not a new protocol.

Add a `dump` module to `crates/asperitas-logging` that compiles for both no_std (device producer) and host (test + example), defining two record bodies in the existing house style (`BOOT`/`STATUS` key=value precedent in `console.rs`):

- `AUDIO block_index=<i> n_of_n=<k> chunk_i=<j> b64=<base64>` — one chunk of one block, body length chosen so the base64 payload fills exactly 200 bytes (150 raw bytes per record).
- `AUDEND block_index=<i> n_of_n=<k> bytes=<n> crc16=<crc>` — closes a block; the CRC covers the **raw** concatenated bytes, which is what makes a lost whole *record* provable rather than merely suspicious. `frame.rs` documents that a record whose leading `~` is lost produces no integrity failure at all, so per-record CRC alone cannot prove absence; block sequence plus `n_of_n` can.

Also here, because it is arithmetic rather than firmware: the ring-capacity policy the dump writer uses, expressed as a pure function with a host test, so the invariant that keeps live log traffic lossless is checked in CI instead of discovered at the bench. A dump must never consume the last `MAX_FRAME` bytes of the 2048-byte pipe; when there is less room than that the writer waits instead of writing, which is what lets AC #3 of the parent be a real claim about `dropped_full`.

Finally, a host-only reassembler example, so the format is proven by a program before any board exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A `dump` module in `crates/asperitas-logging` compiles for both no_std and host, defines `AUDIO block_index/n_of_n/chunk_i/b64` and `AUDEND block_index/n_of_n/bytes/crc16` bodies inside the existing v1 frame grammar, and leaves `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, and the CRC polynomial untouched.
- [ ] #2 Base64 encode and decode are implemented without allocation and without `std`; a host property test compares them against reference vectors over random inputs including every alphabet edge case and the exact 200-byte body boundary, and the payload decodes to exactly 150 raw bytes per full-size record.
- [ ] #3 A host assembler accepts a block only when all `n_of_n` chunks arrived and the CRC-16 of the reassembled raw bytes matches `AUDEND`; on failure it names the missing chunk indices rather than returning partially assembled data.
- [ ] #4 Loss is proven by test, not asserted in prose: deleting any whole record from a synthetic stream is detected by sequence, a corrupted body fails the block CRC, and a stream whose leading `~` markers were lost is handled without inventing records or silently accepting data, consistent with the blind spot documented at the top of `frame.rs`.
- [ ] #5 The ring-capacity policy is a pure function the firmware calls, and a host test proves the invariant that a dump writer never consumes the last `MAX_FRAME` bytes of the pipe, so a maximum-size log or status record always has room and `dropped_full` cannot be blamed on the dump.
- [ ] #6 A host test computes useful-bytes-per-wire-byte and wire cost per second of capture from the same constants the encoder uses (targeting 150/228 = 0.66), so the arithmetic quoted in documentation cannot drift away from the code.
- [ ] #7 `examples/dump_reassemble.rs` turns a captured console byte stream into raw PCM plus a manifest, exits non-zero on any incomplete or mismatched block, and is exercised in CI from synthetic streams containing truncation and corruption with no board attached.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Constraints already established (do not re-derive)

- Frame geometry: `MAX_BODY = 200` (frame.rs:93), `PREFIX_LEN = 21`, `TRAILER_LEN = 7`, `MAX_FRAME = 228` (frame.rs:102), `MIN_CR_OFFSET = 26`. Record: `'~' level SP seq(8 hex) SP t_ms(8) SP body '*' crc(4 hex) CRLF`. CRC is CRC-16/CCITT-FALSE via `pub fn crc16_ccitt(data: &[u8]) -> u16` (frame.rs:144), covering everything after `~` up to but excluding `*`.
- Encoder entry points to reuse, not reimplement: `encode(level, seq, now_ms, body, out) -> Encoded` (frame.rs:185) and `write_whole(frame, free_capacity, write) -> bool` (frame.rs:306). `write_whole` exists because `Pipe::try_write` short-writes at every ring wrap even on an empty ring (measured: 200-byte write into a 512-byte-empty ring accepts 112 bytes).
- Transport reality: `LOG_PIPE` is `embassy_sync::pipe::Pipe<CriticalSectionRawMutex, LOG_PIPE_SIZE>` with `LOG_PIPE_SIZE = 2048` (lib.rs:196/214, behind feature `log-usb`). It holds nine max-size frames. Producer path is `emit()` (lib.rs:264): one critical section per record, atomic-or-drop with `dropped_full` / `bytes_dropped` counters surfaced in `STATUS`. Drainer is `usb::run()` (usb.rs:239): `DRAIN_BUF_SIZE = 256` per wakeup, `MAX_PACKET_SIZE = 64`, embassy-usb 0.6.0 CDC-ACM on `USB_OTG_FS`, i.e. USB **full speed**.
- Backpressure mechanism for the waiting writer: `Pipe::ready_send(n)` yields a future ready when at least n bytes are free, then `write_whole` commits atomically. That path does not touch the drop counters, which is exactly why the reserve-headroom rule is needed to protect ordinary log records.
- Base64 alphabet is fully legal under `sanitize_byte` (which only rewrites `< 0x20` and `0x7F`). Ascii85/Z85 are more efficient (0.70 vs 0.66 useful bytes per wire byte) but contain `~` (the record start marker) and `<>` (reserved host-to-device by TASK-032), which weakens resync. Hex is 0.44 and wasteful. Standard base64 with `=` padding wins on being boring and greppable.
- Efficiency is body-length-bound: 150/228 = 0.66 useful. Arithmetic to encode in a test and in docs: mono 16-bit capture at 96,000 B/s needs 640 records/s and ~146 kB/s on the wire; mono 32-bit doubles both to ~292 kB/s, which is marginal against a full-speed-CDC ceiling nobody in this repo has measured. Real sustained drain throughput is measured later in TASK-038.05; until then these are predictions and should be labelled as such.

## Test strategy to copy

- `crates/asperitas-logging/tests/console_frame.rs` (1297 lines) is the model: proptest strategies `arb_body` / `arb_chunk_sizes` / `arb_stream`, canonical golden rows `canonical_row_1..14` with pinned CRCs, and the accounting law `bytes_pushed == sum consumed + discarded_bytes + buffered()`. Mirror the canonical-row discipline for AUDIO/AUDEND with pinned golden bodies.
- `frame` and `console` are ungated modules compiled and tested on host with default features; anything touching `CriticalSectionRawMutex` stays behind `log-usb` because on host it fails at link time (`_critical_section_1_0_acquire`). Keep `dump` pure logic so its tests need no feature gymnastics.
- Cross-check the hand-written base64 against a reference implementation as a dev-dependency-only test if the crate is available offline; otherwise pin golden vectors including every alphabet edge case (all-zero, all-ones, 1- and 2-byte tails, 200-byte boundary).
- `examples/console_decode.rs` shows the expected example shape: read stdin in 4 KiB chunks, print counters plus the accounting law to stderr, exit non-zero only on unreadable input.

## Deliberate non-goals

- No change to `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, the record prefix, or the CRC polynomial. If measurement in TASK-038.05 says the dump is unbearably slow, raising `MAX_BODY` becomes its own v2 ticket with regenerated golden CRCs and updated TASK-030.03 documentation — not a drive-by edit here.
- No serial-port code. The reassembler reads bytes from a file or stdin; owning the device serial port belongs to the TASK-031 runner.
- No COBS/binary framing. TASK-030 chose printable ASCII deliberately so humans can read and grep captures; revisit only with evidence.
<!-- SECTION:NOTES:END -->
