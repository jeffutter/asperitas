---
id: TASK-038.04.03.01
title: >-
  Add the host-tested replay staging core: ping-pong accounting, underrun policy
  and DAC-path CRC
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-10-08 15:39'
updated_date: '2026-10-08 15:54'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-038.04.03
priority: high
ordinal: 137800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: a pure no_std module (asperitas-logging, ungated, e.g. replay.rs) with no hardware types. Owns: the staging geometry as constants (half-buffer bytes, derived from refill latency), a two-half ping-pong state machine (Filling/Ready/Playing per half, callback side pops frames, refill side takes the idle half), end-of-excerpt handling (zero-pad tail, pass complete), an explicit underrun counter with a defined behaviour (output silence, never stale data), and a running CRC-16 over exactly the s16 samples handed to the DAC encoder, finalised at end of pass. Includes a const-evaluated justification table: bytes/s 96,000, half duration, assumed QSPI throughput lower bound, margin factor, asserted at compile time. Acceptance: host tests for the state machine under adversarial refill timing (late refill gives underrun, never reads a half being filled), tail handling for lengths not a multiple of the half size, CRC equals crc16_ccitt of the source PCM after one clean pass, and a proptest over arbitrary callback and refill interleavings. Covers parent AC #4 (numeric justification, no-QSPI-in-callback structure) and AC #5 (CRC accounting) and AC #7.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New ungated no_std module replay.rs in asperitas-logging: HALF_BYTES, HALF_COUNT=2, callback block size constants, and a const-evaluated staging justification (bytes per second, half duration in microseconds, assumed worst-case refill latency, required margin factor) with a compile-time assert that duration >= margin x latency; every input labelled assumed or measured in a comment a reviewer can check
- [ ] #2 Half lifecycle published as data like capture::BlockState: HalfState {Empty, Filling, Ready, Playing} plus transition_ok, with exhaustive tests of every legal and illegal edge; the refill side only claims an Empty half and the callback side only reads a Ready one
- [ ] #3 ReplayCore (atomics, no unsafe beyond what the buffer handoff needs, Sync, host-testable) exposes: refill side - claim_fill() returning the half and the source byte range to read, finish_fill(half, valid_bytes); callback side - take_block(out: &mut [i16; 32]) that returns Played or Underrun, zero-fills the output on underrun, and does not advance the stream on underrun
- [ ] #4 Pass accounting: running CRC-16 over exactly the s16 samples handed out from the source (silence inserted on underrun is excluded), pass complete when pcm_bytes samples were consumed, tail shorter than a block is zero-padded in the output but excluded from the CRC, underrun counter saturates and is readable, finish() yields {crc16, samples, underruns} and 'clean' means underruns == 0
- [ ] #5 Host tests: CRC after one clean pass equals frame::crc16_ccitt of the source for lengths 0, 1 sample, one block, exactly one half, half+1 sample, and a corpus-sized clip; a late refill yields Underrun and a clean-flag of false but still CRC-equal once the data finally arrives; a proptest over arbitrary callback/refill interleavings never lets the callback read a half being filled and never yields a CRC different from the source on a pass with zero underruns
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

New file crates/asperitas-logging/src/replay.rs, registered 'pub mod replay;' next to capture in lib.rs. Model it on capture.rs, which already does exactly this shape: geometry consts derived and const-asserted, a #[repr(u8)] state enum with from_u8/as_u8 and a transition_ok edge function, a firmware AtomicU8 array, and a tests/capture_geometry.rs oracle. Precedent for chained CRC: frame::crc16_ccitt_update with CRC16_INITIAL (chaining is sound, documented there).

Facts: callback block is daisy_embassy BLOCK_LENGTH = 32 samples (audio.rs:13), i.e. 64 mono bytes, 666 us at 48 kHz; replay is mono 16-bit 48 kHz = 96,000 B/s. Choose HALF_BYTES = 8192 (128 blocks, 85.3 ms). Two halves = 16 KiB RAM. Justification table (const): half duration = 8192 / 96000 = 85.3 ms; assumed worst-case refill = scheduling latency of the thread executor under console load (assume 10 ms, ASSUMED, to be replaced by a reading from TASK-038.09) + an 8 KiB read_async transfer priced conservatively at 1 MB/s = 8.2 ms (ASSUMED; QSPI kernel clock is unset so the real rate is unknown, peak ~30 MB/s at 60 MHz quad); required margin 4x => 85.3 >= 4 x 18.2 = 72.8 passes with little room, which is the point: if a bench reading degrades, the assert is where the number changes. Verify these figures yourself when implementing and adjust HALF_BYTES if the arithmetic is off; the requirement is that the table be computed, not typed.

Design:
1. Constants + const asserts (HALF_BYTES multiple of 2*BLOCK_SAMPLES... i.e. of 64; margin assert; half count 2).
2. HalfState enum + transition_ok: Empty->Filling (refill claims), Filling->Ready (finish_fill), Ready->Playing (callback adopts), Playing->Empty (callback drains). Same edge-check helper style as capture::advance.
3. ReplayCore: [AtomicU8;2] states, AtomicU32 for source position handed to refill (next byte offset), valid length per half (AtomicU16/U32), play cursor, running CRC (u16 held in AtomicU32, written only by callback side), underrun counter AtomicU32, pass state. The sample buffers are NOT inside the core (the firmware owns them in AXI SRAM and needs raw pointers for MDMA); instead take_block receives the half's bytes via a closure or the core is generic over a buffer accessor trait; simplest: take_block(&self, halves: &[&[u8]; 2], out) - firmware passes the two slices (unsafe in firmware, documented); host tests pass Vec-backed arrays. Keep unsafe out of the core.
4. Underrun policy: output silence (zeros), do not advance, count, set sticky not-clean. Underrun before the first fill is the same event. Reason: advancing would make the host CRC unreachable and hide the glitch; stalling keeps stream order and makes the report truthful (CRC equal AND underruns==0 is the claim).
5. End: refill side stops claiming when position >= pcm_bytes; last half has valid_bytes < HALF_BYTES; callback zero-pads the final partial block, excludes padding from CRC, then pass complete and further blocks output silence without counting underruns.
6. Result: Finished {crc16, samples, underruns}; usable exactly once.
7. Tests in tests/replay_core.rs plus in-module unit tests: transitions exhaustive over 4x4 states; table asserts; the length cases; interleaving proptest (random sequence of refill/callback actions with delays) with an oracle Vec; CRC compared to frame::crc16_ccitt over the source; no-torn-read check by filling a Filling half with a poison pattern and asserting it never appears in output.

Verify: cargo test -p asperitas-logging; cargo build for thumbv7em (const asserts evaluate in target usize - use u64 intermediates for durations, per the capture.rs note about overflow); clippy. No firmware wiring here (TASK-038.04.03.02). Do not claim QSPI throughput; the assumptions are labelled for bench replacement.
<!-- SECTION:PLAN:END -->
