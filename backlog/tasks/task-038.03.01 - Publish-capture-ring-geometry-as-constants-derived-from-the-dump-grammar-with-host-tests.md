---
id: TASK-038.03.01
title: >-
  Publish capture-ring geometry as constants derived from the dump grammar, with
  host tests
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-11 13:24'
updated_date: '2026-09-11 13:28'
labels:
  - task
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/capture.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/tests/capture_geometry.rs
parent_task_id: TASK-038.03
priority: high
type: task
ordinal: 82500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-038.03 must publish one set of numbers that simultaneously describes an SDRAM capture ring and the console traffic that ring generates. Today those numbers exist in two places that cannot see each other: `asperitas-logging/src/dump.rs` owns the wire grammar (`CHUNK_RAW = 129`, `MAX_CHUNKS_PER_BLOCK = 255`, `MAX_BLOCK_BYTES = 32_895`, `FULL_AUDIO_FRAME_LEN = 227`), while the ring geometry lives only in prose in TASK-038.03's notes. Nothing checks that a 32 KiB ring block is encodable. It is — by exactly zero margin: 32,768 B is 254 full chunks plus a 2-byte tail = 255 chunks, which *is* `MAX_CHUNKS_PER_BLOCK`. Exceed it and `audio_body` refuses at runtime mid-dump (`dump.rs:661`), discovered at the bench with ears on the line, because no code path proves the pair compatible until a capture runs.

This ticket closes that gap where it can actually be checked: a host-visible module in `asperitas-logging` that owns the ring geometry, derives every wire figure from `dump`'s own constants, and const-asserts the two against each other. An off-by-one then fails `cargo build` on the cross target and `cargo test` on the host, with no board attached — which is precisely what TASK-038.03 AC #7 asks for and what its sibling leaves cannot provide from inside `firmware/`, where host tests do not run.

Keeping the numbers here rather than in `rig.rs` also gives TASK-038.04 (QSPI excerpts) and TASK-038.06 (budget documentation) one place to cite, instead of a third copy of `32768`.

Scope guard: this ticket adds no firmware code, touches no hardware, and changes no existing constant. `dump.rs` and `frame.rs` stay byte-identical; the module reads their public constants and never redefines them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crates/asperitas-logging/src/capture.rs exists and is exported as pub mod, ungated by log-usb, so it compiles on both the host and thumbv7em-none-eabihf. It owns every capture-ring number: sample rate, bytes per audio callback, ring block bytes, ring block count, ring bytes, bytes per second.
- [ ] #2 No figure duplicates the wire grammar by hand. chunks-per-block, records-per-block and the per-block wire-byte budget are computed from dump::CHUNK_RAW, dump::MAX_CHUNKS_PER_BLOCK, dump::FULL_AUDIO_FRAME_LEN, dump::MAX_AUDEND_BODY_LEN and the frame prefix/trailer lengths, so widening a header moves the ring numbers instead of silently desynchronising them.
- [ ] #3 Compile-time asserts hold: RING_BLOCK_BYTES <= dump::MAX_BLOCK_BYTES, chunks_per_block() <= dump::MAX_CHUNKS_PER_BLOCK, RING_BLOCK_BYTES % CALLBACK_BYTES == 0, and both RING_BLOCK_BYTES and RING_BYTES are powers of two. A change that pushes a ring block past the grammar ceiling fails cargo build, not a bench dump.
- [ ] #4 tests/capture_geometry.rs derives and pins, from the published constants: 512 callbacks per block, 255 AUDIO chunks plus one AUDEND giving 256 records, 1,024 blocks, 96,000 bytes/s footprint, 349 s floor and 349,525,333 microseconds exact ring capacity, a 57,788-byte worst-case wire budget per block, and a useful fraction of 567 per 1000 matching the pinned encoder efficiency.
- [ ] #5 A pure BlockState contract (Free -> Filling -> Full -> Dumping -> Free) is published with a total transition_ok function plus as_u8/from_u8, and an exhaustive host test over all sixteen pairs asserts exactly the four legal edges, so the firmware ring and its tests share one state machine.
- [ ] #6 The five-minute capture window TASK-019.03 asks for fits the ring: expected_blocks(300) == 878 < RING_BLOCKS, asserted in the host test, so ring-full is a backstop rather than the normal end of a run.
- [ ] #7 cargo fmt --all --check, cargo test -p asperitas-logging --all-targets, cargo clippy --workspace --all-targets -- -D warnings, cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings, and cd firmware && cargo build --release --features seed3 all pass, and dump.rs plus frame.rs are untouched.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Three files, one of them new code:

- `crates/asperitas-logging/src/capture.rs` (new) — the geometry and the state contract.
- `crates/asperitas-logging/src/lib.rs` — add `pub mod capture;` beside `pub mod dump;` (ungated, like `dump`).
- `crates/asperitas-logging/tests/capture_geometry.rs` (new) — the derivation the CI gate runs.

Do not put these numbers in `firmware/src/bin/rig.rs`. The firmware package is a separate workspace with no host-test target, so a literal there is checked only by a cross build that cannot compare it against anything. Here the same `const` assertions run on both targets *and* a host test asserts the resulting figures.

## Constants to publish

Hardware-derived inputs, each documented with where it comes from:

```rust
pub const SAMPLE_RATE_HZ: u32 = 48_000;          // daisy-embassy AudioConfig::default()
pub const FRAMES_PER_CALLBACK: usize = 32;       // daisy_embassy::audio::BLOCK_LENGTH
pub const CAPTURE_SAMPLES_PER_CALLBACK: usize = FRAMES_PER_CALLBACK;  // one int16 per frame
pub const CALLBACK_BYTES: usize = CAPTURE_SAMPLES_PER_CALLBACK * 2;   // 64
```

`FRAMES_PER_CALLBACK` is a copy of a driver constant on purpose: `asperitas-logging` must stay dependency-free and host-testable, so it cannot name `daisy_embassy`. Say so in the doc comment, and require the *firmware* side to close the loop with `const _: () = assert!(capture::CALLBACK_BYTES == daisy_embassy::audio::HALF_DMA_BUFFER_LENGTH * 2);` — that assertion belongs to TASK-038.03.02 and is called out there too. One owner of the value, one check that the copy is honest.

Ring geometry, all `pub const`:

```rust
pub const CAPTURE_BYTES_PER_SAMPLE: usize = 2;    // mono 16-bit, AC #4
pub const RING_BLOCK_BYTES: usize = 32_768;
pub const RING_BLOCKS: usize = 1_024;
pub const RING_BYTES: usize = RING_BLOCK_BYTES * RING_BLOCKS;         // 33_554_432
pub const BYTES_PER_SECOND: usize = SAMPLE_RATE_HZ as usize * CAPTURE_BYTES_PER_SAMPLE; // 96_000
```

Derived, all `pub const fn` so both a `const` context and a test can call them:

| helper | value at the defaults above |
|---|---|
| `callbacks_per_block()` | 512 |
| `samples_per_block()` | 16_384 |
| `chunks_per_block()` | `(RING_BLOCK_BYTES + CHUNK_RAW - 1) / CHUNK_RAW` = 255 |
| `records_per_block()` | `chunks_per_block() + 1` = 256 (one `AUDEND`) |
| `tail_chunk_bytes()` | `RING_BLOCK_BYTES % CHUNK_RAW` = 2 |
| `wire_bytes_per_block()` | see below, 57_788 |
| `ring_seconds_floor()` | `RING_BYTES / BYTES_PER_SECOND` = 349 |
| `ring_duration_micros()` | `RING_BYTES * 1_000_000 / BYTES_PER_SECOND` = 349_525_333 |
| `expected_blocks(seconds)` | `seconds * BYTES_PER_SECOND / RING_BLOCK_BYTES` |

`wire_bytes_per_block()` must be assembled from the encoder's own constants, never a literal:

```
(full_chunks * FULL_AUDIO_FRAME_LEN)
  + tail_frame_len
  + PREFIX_LEN + MAX_AUDEND_BODY_LEN + TRAILER_LEN
where full_chunks   = RING_BLOCK_BYTES / CHUNK_RAW            // 254
      tail_frame_len = PREFIX_LEN + AUDIO_HEADER_LEN
                     + encoded_len(tail_chunk_bytes())
                     + TRAILER_LEN                            // 59
```

`encoded_len` is `frame`'s base64 sizing; if it is not reachable, widen it to `pub(crate)` the way TASK-038.02 widened `write_hex`/`write_decimal` rather than re-deriving `4 * ceil(n/3)` a second time. Note in the doc comment that this is the *worst-case* budget: `MAX_AUDEND_BODY_LEN` is the maximum `AUDEND` body, and a real one carrying five decimal digits is shorter.

## The block-state contract

Publish the ownership machine AC #4 describes, as data rather than as prose in a firmware comment:

```rust
#[repr(u8)]
pub enum BlockState { Free = 0, Filling = 1, Full = 2, Dumping = 3 }

pub const fn transition_ok(from: BlockState, to: BlockState) -> bool
```

Legal edges: `Free -> Filling` (producer claims), `Filling -> Full` (producer publishes), `Full -> Dumping` (consumer claims), `Dumping -> Free` (consumer releases). Everything else false, including both self-transitions and any skip. Encode it as a match over the four-by-four product so the compiler checks exhaustion, and document the two rules the transitions exist to enforce: the producer never writes a block that is not `Free`, and the consumer never reads a block that is not `Full`.

Ship `BlockState::from_u8`/`as_u8` too — the firmware stores these in an `AtomicU8` array and needs the mapping without a `match` at the call site.

## Compile-time gates

In `capture.rs`, as `const _: () = assert!(...)` (the idiom `dump.rs:474-489` already uses):

1. `RING_BLOCK_BYTES <= dump::MAX_BLOCK_BYTES` — the one that matters. Zero margin today (32,768 vs 32,895), so this is the difference between a CI failure and a runtime refusal in the middle of a bench dump.
2. `chunks_per_block() <= dump::MAX_CHUNKS_PER_BLOCK` — the `n` field is two hex digits; state the modulus relationship explicitly.
3. `RING_BLOCK_BYTES % CALLBACK_BYTES == 0` — capture granularity must divide the block or blocks fill unevenly.
4. `RING_BYTES.is_power_of_two()` and `RING_BLOCK_BYTES.is_power_of_two()` — wrap arithmetic stays a mask.
5. `RING_BLOCKS <= u16::MAX as usize` — the firmware keeps the per-block state array in internal RAM, which has roughly 69 KB free.

Comment 1 on the gate explaining *why* it is zero-margin-tolerant: the block size is chosen to maximise useful bytes per `AUDEND` and to minimise the state array, and the gate is what makes that safe. Anyone who wants a bigger ring changes one constant and the gate tells them whether the grammar allows it.

## Host test: `tests/capture_geometry.rs`

Follow the style contract in `tests/console_frame.rs:1-22` — `///` docs on helpers, `// ── Section ──` banners, third-person indicative names, no `ProptestConfig`. This file is exhaustive arithmetic, so proptest earns nothing here; assert equalities.

Pin, each derived from the published constants and compared against a literal expectation written once:

- `a_block_is_512_callbacks_of_mono_16_bit` — `callbacks_per_block() == 512`.
- `a_block_needs_255_chunks_and_256_records` — `chunks_per_block() == 255`, `records_per_block() == 256`, and `tail_chunk_bytes() == 2`. Name in the doc comment that 255 equals `MAX_CHUNKS_PER_BLOCK` exactly, which is the point.
- `the_ring_holds_349_point_5_seconds` — `RING_BLOCKS == 1_024`, `ring_seconds_floor() == 349`, `ring_duration_micros() == 349_525_333`, and `BYTES_PER_SECOND == 96_000`.
- `the_wire_budget_for_one_block_matches_the_encoder` — `wire_bytes_per_block() == 57_788`, recomputed inline in the test from `FULL_AUDIO_FRAME_LEN` etc., so the test and the module agree by construction and disagree with reality loudly.
- `useful_fraction_of_the_wire_is_the_pinned_efficiency` — `RING_BLOCK_BYTES * 1000 / wire_bytes_per_block() == 567`, tying this module to `published_efficiency_matches_the_encoder` (`tests/console_dump.rs:946`) so nobody quotes 0.487 or 0.660 again.
- `a_full_capture_window_fits_the_ring` — `expected_blocks(300) == 878` and `expected_blocks(300) < RING_BLOCKS`, i.e. a five-minute run (TASK-019.03 AC #2) never fills the ring, and the ring-full stop is the backstop rather than the normal end.
- `illegal_block_transitions_are_refused` — enumerate all 16 pairs, assert the 4 legal ones, and assert the producer-consumer invariants follow from the table.
- `wrap_arithmetic_stays_inside_the_ring` — for block indices `0`, `1`, `1023`, and the wrapped `1024 % RING_BLOCKS`, byte offsets stay within `RING_BYTES` and consecutive blocks tile it exactly once.

## Verification

```
cargo fmt --all --check
cargo test -p asperitas-logging --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings
cd firmware && cargo build --release --features seed3
```

The last one proves the module costs nothing on the cross target even though nothing in `firmware/` imports it yet. Record the `.bss`/`.text` delta from that build in the finalization notes; expect zero.

## Non-goals

- No firmware code, no `unsafe`, no pointer arithmetic — the ring is carved in TASK-038.03.02.
- No change to `MAX_BODY`, `MAX_FRAME`, `CHUNK_RAW`, the frame prefix, or the CRC polynomial. If the zero margin feels wrong, the fix is a smaller `RING_BLOCK_BYTES` here, not a wider header.
- No runtime configurability of any of it. Every value is `const`; the wire format and the ring must agree at compile time, which is the entire reason this module exists.
<!-- SECTION:PLAN:END -->
