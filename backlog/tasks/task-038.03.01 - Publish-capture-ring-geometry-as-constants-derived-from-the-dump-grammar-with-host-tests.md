---
id: TASK-038.03.01
title: >-
  Publish capture-ring geometry as constants derived from the dump grammar, with
  host tests
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-11 13:24'
updated_date: '2026-09-11 14:49'
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
- [ ] #3 Compile-time asserts hold: RING_BLOCK_BYTES <= dump::MAX_BLOCK_BYTES (127 B of slack today), chunks_per_block() <= dump::MAX_CHUNKS_PER_BLOCK (equal today, zero margin, which makes this the load-bearing one), RING_BLOCK_BYTES.is_multiple_of(CALLBACK_BYTES), both RING_BLOCK_BYTES and RING_BYTES are powers of two, RING_BLOCKS <= u16::MAX, and expected_blocks(CAPTURE_WINDOW_SECONDS) < RING_BLOCKS. Every duration helper carries an explicit width -- ring_duration_micros returns u64, ring_seconds_floor u32 -- with each intermediate product taken in u64, because const eval runs in the target's environment where usize is 32 bits: the usize form of RING_BYTES * 1_000_000 / BYTES_PER_SECOND builds clean on the host and fails the cross build with E0080. A change that pushes a ring block past the grammar ceiling fails cargo build, not a bench dump.
- [ ] #4 tests/capture_geometry.rs derives and pins, from the published constants: 512 callbacks per block, 255 AUDIO chunks plus one AUDEND giving 256 records, 1,024 blocks, 96,000 bytes/s footprint, 349 s floor and 349,525,333 microseconds exact ring capacity, and an exact -- not worst-case -- 57,788-byte wire budget per block. That budget is confirmed by a second oracle which encodes a real 255-chunk block through dump::audio_record and dump::audend_record and sums the frame lengths the codec actually produced. The block-level useful fraction is pinned at 567 per 1000, named as distinct from and legitimately below the 568 per-record efficiency pinned by published_efficiency_matches_the_encoder.
- [ ] #5 A pure BlockState contract (Free -> Filling -> Full -> Dumping -> Free) is published with a total transition_ok function plus as_u8/from_u8, where from_u8 returns Option<BlockState> rather than reading an unrecognised status byte as Free, and an exhaustive host test over all sixteen pairs asserts exactly the four legal edges, so the firmware ring and its tests share one state machine.
- [ ] #6 The five-minute capture window TASK-019.03 asks for fits the ring: expected_blocks(300) == 878 < RING_BLOCKS, asserted in the host test, so ring-full is a backstop rather than the normal end of a run.
- [ ] #7 Everything lefthook and CI enforce passes with these files added: cargo fmt --all --check; cargo test -p asperitas-logging --all-targets; cargo test --workspace; cargo clippy --workspace --all-targets -- -D warnings; the same with --features asperitas-pod/pod-hw; cargo clippy -p asperitas-logging --features log-usb --lib and --features log-defmt --lib, each with -D warnings; RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps in both the default and --all-features forms; and cd firmware && cargo build --release --features seed3. dump.rs and frame.rs are untouched.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Validated already — do not re-derive these numbers

A prototype of exactly the module below was written into a throwaway copy of the repo and run
against every gate CI enforces. Everything in this plan is measured, not estimated:

- 11 host tests pass, including an oracle test that encodes a real 255-chunk block through
  dump::audio_record / dump::audend_record and sums the frames the codec actually produced:
  **57,788 bytes**, matching the derived budget exactly.
- cargo build -p asperitas-logging --target thumbv7em-none-eabihf: green. So are both firmware
  release builds (seed3, and the RTT-only form), so the module costs nothing on the cross target.
- cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, clippy for
  --features log-usb and --features log-defmt each with -D warnings, and RUSTDOCFLAGS="-D warnings"
  cargo doc --workspace --no-deps in both default and --all-features forms: green.
- cargo test --workspace: green (263 tests), with dump.rs and frame.rs byte-identical to HEAD.
- The 32-bit trap is confirmed rather than theorised. Written as a usize product,
  RING_BYTES * 1_000_000 / BYTES_PER_SECOND builds clean on the host and fails the cross build with
  error[E0080]: attempt to compute 33554432_usize * 1000000_usize, which would overflow. Host tests
  would pass; AC #7's firmware build would not. That is the whole reason the width rule below exists.

## Shape

Three files:

- crates/asperitas-logging/src/capture.rs (new) — geometry, derived figures, compile-time gates,
  and the block-state contract.
- crates/asperitas-logging/src/lib.rs — add pub mod capture; in the "Ungated modules — pure logic,
  no hardware types, host-testable by default" block beside pub mod dump; (lib.rs:201-205). Ungated,
  like dump. Keep it free of critical-section and embassy-time types: those fail at link time on the
  host, which is why LOG_PIPE_SIZE is an ungated bare usize while LOG_PIPE stays gated.
- crates/asperitas-logging/tests/capture_geometry.rs (new) — the derivation the CI gate runs.

Why here and not in a new crate or in rig.rs: the module's entire reason to exist is its coupling to
the wire grammar, so putting it anywhere else replaces one dependency with a restated constant. The
firmware package is a separate workspace with no host-test target, so a literal there is checked only
by a cross build with nothing to compare against. Here the same consts are evaluated on both targets
and a host test asserts the result.

## Published surface

Hardware inputs (each documented with where it comes from, and that it is a deliberate copy):

    pub const SAMPLE_RATE_HZ: u32 = 48_000;                  // daisy-embassy AudioConfig::default()
    pub const CAPTURE_BYTES_PER_SAMPLE: usize = 2;           // mono int16
    pub const FRAMES_PER_CALLBACK: usize = 32;               // daisy_embassy::audio::BLOCK_LENGTH
    pub const CALLBACK_BYTES: usize = FRAMES_PER_CALLBACK * CAPTURE_BYTES_PER_SAMPLE;  // 64

Ring geometry:

    pub const RING_BLOCK_BYTES: usize = 32_768;
    pub const RING_BLOCKS: usize = 1_024;
    pub const RING_BYTES: usize = RING_BLOCK_BYTES * RING_BLOCKS;                       // 33_554_432
    pub const BYTES_PER_SECOND: usize = SAMPLE_RATE_HZ as usize * CAPTURE_BYTES_PER_SAMPLE; // 96_000
    pub const CAPTURE_WINDOW_SECONDS: u32 = 300;             // TASK-019.03's five minutes

Derived, all pub const fn so both a const context and a test can call them:

| helper | value | notes |
|---|---|---|
| callbacks_per_block() | 512 | |
| samples_per_block() | 16_384 | |
| full_chunks_per_block() | 254 | floor |
| tail_chunk_bytes() | 2 | 0 if the block divided evenly |
| chunks_per_block() | 255 | RING_BLOCK_BYTES.div_ceil(dump::CHUNK_RAW) |
| records_per_block() | 256 | chunks + one AUDEND |
| tail_frame_bytes() | 59 | PREFIX_LEN + AUDIO_HEADER_LEN + encoded_len(tail) + TRAILER_LEN; **must be 0 when tail_chunk_bytes() == 0**, so the formula survives a block size that divides evenly |
| audend_frame_bytes() | 71 | PREFIX_LEN + MAX_AUDEND_BODY_LEN + TRAILER_LEN |
| wire_bytes_per_block() | 57_788 | see "exact, not worst-case" |
| useful_fraction_per_mille() -> u32 | 567 | u64 division |
| ring_seconds_floor() -> u32 | 349 | |
| ring_duration_micros() -> u64 | 349_525_333 | (RING_BYTES as u64 * 1_000_000) / BYTES_PER_SECOND as u64 |
| expected_blocks(seconds: u32) -> usize | 878 at 300 | multiply in u64 |

dump::encoded_len is already pub (dump.rs:190) and clamps rather than wrapping, so use it directly.
The earlier fallback in this plan — widen it the way TASK-038.02 widened write_hex — is unnecessary
and must not be done. Note also that frame::write_hex and write_decimal are pub(crate)
(frame.rs:342, 359): reachable from capture.rs, **not** from tests/. Nothing in the test may reach
for them; it recomputes from the public PREFIX_LEN / TRAILER_LEN / MAX_AUDEND_BODY_LEN instead.

## Three decisions the code has to get right

**1. Integer widths follow the target, not the host.** Const evaluation happens in the target's
environment: usize is 32 bits on thumbv7em. Byte offsets and counts stay usize — the ring is 33.5 MB,
which fits u32, so nothing becomes unaddressable — while every duration is explicit-width with its
intermediate products taken in u64. Document this on the module, because it looks arbitrary otherwise.
expected_blocks also multiplies in u64: seconds * BYTES_PER_SECOND passes u32::MAX near 44,700 s, and
the const evaluator rejects the 32-bit form outright rather than leaving it behind a debug_assert.

**2. wire_bytes_per_block() is exact, not a worst case.** Every numeric field renders fixed-width and
zero-padded ("exactly dst.len() digits", frame.rs:332, 351), so an AUDIO record is always 227 B and an
AUDEND is always 43 body -> 71 wire whatever it carries; bytes="32768" is exactly BYTES_DEC_DIGITS=5.
Drop the worst-case hedge from the doc comment — writing "worst case" where the truth is "always"
teaches the next reader to expect a typical dump to come in under it, which it never does.

**3. 567 is block-level and must be named as such.** published_efficiency_matches_the_encoder
(tests/console_dump.rs:946) pins 568, and that figure is per record (CHUNK_RAW inside one 227-byte
frame). 567 additionally pays for the AUDEND and the 2-byte tail chunk. They are not equal and never
will be; a test that implies otherwise reads as contradicting a pinned document. Pin both in the new
file and assert strictly less-than between them so they cannot drift into false agreement. Also worth
one doc line: dump.rs's module doc quotes 0.568 (line 85) and 0.487 for named payloads (line 94) —
this module adds a third figure at a third granularity, and saying which is which is cheaper than
rediscovering it.

## Compile-time gates

Use the idiom the crate already has — plain const _: () = assert!(cond, "literal message")
(dump.rs:474-489, frame.rs:114-140). No static_assertions dependency; assert! with a message works in
const context on rustc 1.97.1, as do div_ceil and is_power_of_two.

1. RING_BLOCK_BYTES <= dump::MAX_BLOCK_BYTES — 127 B of slack today (32,895 - 32,768).
2. chunks_per_block() <= dump::MAX_CHUNKS_PER_BLOCK — **equal today, zero margin**. This is the
   load-bearing gate; say so, and do not describe gate 1 as zero-margin. The two differ in tightness
   and the original text of this plan conflated them.
3. RING_BLOCK_BYTES.is_multiple_of(CALLBACK_BYTES) — write it that way, not % .. == 0: clippy's
   manual_is_multiple_of lint denies it under -D warnings. is_multiple_of is const-usable; dump.rs's
   own encoded_len already uses it.
4. Both RING_BLOCK_BYTES and RING_BYTES .is_power_of_two() — wrap arithmetic stays a mask. kfifo,
   ringbuf and rtrb all make this a documented requirement, so it is correctness rather than style.
5. RING_BLOCKS <= u16::MAX — the per-block state array lives in internal RAM (~69 KB free).
6. expected_blocks(CAPTURE_WINDOW_SECONDS) < RING_BLOCKS — the five-minute window stops fitting only
   if someone shrinks the ring, and that should cost a build, not a bench run. The rig may raise the
   window by option_env! override; its own assert in rig.rs guards that, not this one.

Say in RING_BLOCK_BYTES' doc comment that 32 KiB is provably optimal rather than chosen by taste: the
chunk-count ceiling makes 32,895 B the widest describable block, the next power of two (64 KiB) needs
509 chunks and is refused twice over, and powers of two are forced by gate 4 — which leaves 32 KiB as
the only candidate. Growing further buys little anyway: useful fraction rises slowly with block size
(8 KiB -> 564, 16 KiB -> 565, 32 KiB -> 567 per 1000) while the state array doubles. Those three
figures were computed from the encoder, not guessed. And note that RING_BYTES is 32 MiB, half the
Seed3's 64 MB SDRAM (docs/reference/daisy-seed3.md:17) — the number TASK-038.04 and TASK-038.06 have
to quote alongside this module.

## Block-state contract

    #[repr(u8)] pub enum BlockState { Free = 0, Filling = 1, Full = 2, Dumping = 3 }
    impl BlockState { pub const ALL: [BlockState; 4]; pub const fn as_u8(self) -> u8;
                      pub const fn from_u8(value: u8) -> Option<BlockState>; }
    pub const fn transition_ok(from: BlockState, to: BlockState) -> bool

Legal edges: Free->Filling (producer claims), Filling->Full (producer publishes), Full->Dumping
(consumer claims), Dumping->Free (consumer releases). Write transition_ok as a match over the 4x4
product with **sixteen explicit arms and no wildcard**, so adding a fifth state fails the build
instead of quietly returning false. Document the two rules the edges enforce — the producer writes
only a block found Free, the consumer reads only one found Full — which is the same invariant a
Disruptor claim/publish sequence documents; one line citing that is enough.

from_u8 returns Option, never unwrap_or(Free): a corrupted status byte that silently reads as Free
hands a block carrying audio to a writer, the one failure this machine exists to prevent. Hand-write
the mapping rather than reaching for num_enum or strum — the crate keeps its dependency surface at
log plus cortex-m, and dev-deps never reach firmware/ anyway. The firmware stores these in an
AtomicU8 array; core::sync::atomic::AtomicU8 is natively lock-free on ARMv7-M via LDREXB/STREXB, so
portable-atomic is not needed and stays out.

## Host test: tests/capture_geometry.rs

Follow the style contract at the head of tests/console_frame.rs: /// docs on helpers, section banners
over one idea each, third-person indicative names. No proptest — this is exhaustive arithmetic and a
random sampler can only re-discover the single point it would assert on.

Each file under tests/ is its own crate and there is no tests/common module here, so copy the
audio_wire / audend_wire helpers from tests/console_dump.rs:600,627 (~15 lines each) rather than
inventing a shared module no other test file wants. Their return value can be just the frame length.

Tests, each pinning literals written once in the test file:

- a_block_is_512_callbacks_of_mono_16_bit — 48_000, 2, 64, 32_768, 512, 16_384, 96_000.
- a_block_needs_255_chunks_and_256_records — 254, 2, 255, 256; then
  assert_eq!(chunks_per_block(), dump::MAX_CHUNKS_PER_BLOCK) as the zero-margin fact, and a separate
  assert that MAX_BLOCK_BYTES - RING_BLOCK_BYTES == 127 so the byte margin is pinned too and neither
  gate's tightness can be quietly lost.
- the_ring_holds_349_point_5_seconds — 1_024, 33_554_432, 349, 349_525_333.
- one_block_costs_57788_wire_bytes — recompute tail_frame and audend_frame inline from the public
  constants, assert 59, 71, and 57_788.
- a_real_255_chunk_block_costs_what_the_module_claims — **the oracle**. Build a 32,768-byte payload,
  emit 254 chunks of CHUNK_RAW plus the 2-byte tail with dump::audio_record, close with
  dump::audend_record(total_bytes = 32768), sum the returned Encoded::len values, and compare against
  capture::wire_bytes_per_block(). Assert offset == RING_BLOCK_BYTES first, so a loop that silently
  under-covers the block cannot produce a matching total by accident. This is what makes the test
  independent of the module's own arithmetic; comparing the module against itself proves nothing.
- useful_fraction_is_567_per_mille_at_block_level — pin 567, recompute it inline, then pin 568 as the
  per-record figure and assert 567 < 568.
- a_five_minute_capture_fits_the_ring — expected_blocks(300) == 878 and < RING_BLOCKS.
- only_the_four_hand_off_edges_are_legal — loop BlockState::ALL x BlockState::ALL (16 pairs), compare
  each against an explicit four-edge table, and count that exactly four came out legal.
- the_machine_is_one_cycle_with_no_shortcut_to_full — no self-transition for any state, and the
  producer/consumer rules asserted as consequences (Free->Full, Free->Dumping, Full->Filling,
  Dumping->Filling all refused).
- state_round_trips_through_u8_and_refuses_garbage — round-trip all four, then from_u8(4) and
  from_u8(u8::MAX) are None, and the discriminants are exactly [0,1,2,3].
- wrap_arithmetic_stays_inside_the_ring — for indices 0, 1, 512, RING_BLOCKS-1 the byte range ends at
  or before RING_BYTES; 1024 % RING_BLOCKS == 0; RING_BLOCKS * RING_BLOCK_BYTES == RING_BYTES.

## Verification

Run everything lefthook and CI actually enforce — AC #7 now lists the full set, which is wider than
the five commands this plan used to name:

    cargo fmt --all --check
    cargo test -p asperitas-logging --all-targets
    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
    cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings
    cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
    cargo build -p asperitas-logging --target thumbv7em-none-eabihf   # fast 32-bit signal
    cd firmware && cargo build --release --features seed3
    cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt"

Doc links are the sneaky one: broken_intra_doc_links and private_intra_doc_links are denied at the
workspace root and RUSTDOCFLAGS promotes anything else. From capture.rs you may link only dump and
frame's public items — CHUNK_RAW, MAX_CHUNKS_PER_BLOCK, MAX_BLOCK_BYTES, FULL_AUDIO_FRAME_LEN,
FULL_AUDIO_BODY_LEN, MAX_AUDEND_BODY_LEN, AUDIO_HEADER_LEN, encoded_len, max_raw_for, BodyError,
MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN. FULL_B64_CHARS, AUDEND_PREFIX, the SEP_* templates,
write_hex and write_decimal are private: refer to them in prose or single backticks, never as a link.
There is no #![warn(missing_docs)] anywhere, so doc quality here is convention-driven — match dump.rs,
and remember asperitas-cli does not depend on this crate, so the host tool inherits none of it.

Record the .text/.bss delta from the firmware release build in the finalization notes; expect zero,
since nothing in firmware/ imports capture.rs yet.

## Corrections made while planning this ticket

Two assertions elsewhere in the tree could never have compiled. Fixed at the source rather than left
for the executor to hit:

- TASK-038.03.02 AC #4 and its plan snippet required
  capture::CALLBACK_BYTES == daisy_embassy::audio::HALF_DMA_BUFFER_LENGTH * 2. That is 64 == 128.
  HALF_DMA_BUFFER_LENGTH is 64 u32 words per callback (32 stereo frames); CALLBACK_BYTES is 64 **bytes**
  of one mono channel. Both tickets now specify
  capture::FRAMES_PER_CALLBACK == daisy_embassy::audio::BLOCK_LENGTH, compared in samples, with the
  coincidence spelled out so nobody "fixes" it back to a passing-but-meaningless bytes==words compare.
  This is the check this ticket's SAMPLE_RATE_HZ / FRAMES_PER_CALLBACK doc comments promise, so the
  promise had to change too — hence the wording above.
- TASK-038.03 AC #7 still reads "219 chunks per block". Left alone deliberately: that ticket's
  Implementation Notes already record 219 as stale arithmetic from the retracted 150-raw-bytes-per-record
  assumption and state that its own note governs over the criterion text. Do not assert 219 anywhere.

Also corrected in place above: AC #3's two grammar gates have different tightness (127 B of slack vs
zero chunks), AC #4 called an exact figure a worst case and implied 567 matched the pinned 568, and
AC #7 under-listed the enforced gates. All three are now accurate.

## Non-goals

- No firmware code, no unsafe, no pointer arithmetic — the ring itself is carved in TASK-038.03.02.
- No change to MAX_BODY, MAX_FRAME, CHUNK_RAW, the frame prefix, or the CRC polynomial. If the zero
  chunk margin feels wrong, the fix is a smaller RING_BLOCK_BYTES here, not a wider header there.
- No runtime configurability. Every value is const; the wire format and the ring agreeing at compile
  time is the entire reason this module exists.
- Do not add a tests/common module, a static_assertions dependency, or num_enum / strum.
<!-- SECTION:PLAN:END -->
