---
id: TASK-038.04.03.01
title: >-
  Add the host-tested replay staging core: ping-pong accounting, underrun policy
  and DAC-path CRC
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-08 15:39'
updated_date: '2026-10-08 16:47'
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
- [x] #1 New ungated no_std module replay.rs in asperitas-logging: HALF_BYTES, HALF_COUNT=2, callback block size constants, and a const-evaluated staging justification (bytes per second, half duration in microseconds, assumed worst-case refill latency, required margin factor) with a compile-time assert that duration >= margin x latency; every input labelled assumed or measured in a comment a reviewer can check
- [x] #2 Half lifecycle published as data like capture::BlockState: HalfState {Empty, Filling, Ready, Playing} plus transition_ok, with exhaustive tests of every legal and illegal edge; the refill side only claims an Empty half and the callback side only reads a Ready one
- [x] #3 ReplayCore (atomics, no unsafe beyond what the buffer handoff needs, Sync, host-testable) exposes: refill side - claim_fill() returning the half and the source byte range to read, finish_fill(half, valid_bytes); callback side - take_block(out: &mut [i16; 32]) that returns Played or Underrun, zero-fills the output on underrun, and does not advance the stream on underrun
- [x] #4 Pass accounting: running CRC-16 over exactly the s16 samples handed out from the source (silence inserted on underrun is excluded), pass complete when pcm_bytes samples were consumed, tail shorter than a block is zero-padded in the output but excluded from the CRC, underrun counter saturates and is readable, finish() yields {crc16, samples, underruns} and 'clean' means underruns == 0
- [x] #5 Host tests: CRC after one clean pass equals frame::crc16_ccitt of the source for lengths 0, 1 sample, one block, exactly one half, half+1 sample, and a corpus-sized clip; a late refill yields Underrun and a clean-flag of false but still CRC-equal once the data finally arrives; a proptest over arbitrary callback/refill interleavings never lets the callback read a half being filled and never yields a CRC different from the source on a pass with zero underruns
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d0ab084. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Nothing pre-existed at HEAD 8a46ed0; built from scratch.

- crates/asperitas-logging/src/replay.rs (ungated, registered in lib.rs next to excerpt). BLOCK_SAMPLES comes from capture::FRAMES_PER_CALLBACK and BYTES_PER_SECOND from capture::SAMPLE_RATE_HZ, so capture and replay cannot disagree about the callback. HALF_BYTES = 8192, HALF_COUNT = 2. Justification table is const fns with u64 intermediates: half 85 333 us >= 4 x (10 000 us ASSUMED scheduling + 8 192 us transfer at ASSUMED 1 MB/s) = 72 768 us, const-asserted; 4096 would fail (42.7 ms vs 56.4 ms). Also asserted: half is whole 64-byte blocks, whole 32-byte cache lines, count == 2.
- HalfState {Empty, Filling, Ready, Playing} + from_u8/as_u8 + 16-arm transition_ok, same shape as capture::BlockState; every transition goes through a CAS helper that debug-asserts the edge is legal.
- Design choice vs the plan: the core OWNS the two halves (UnsafeCell, repr(align(32)) per half) instead of taking slices from the firmware. That puts the 'callback never reads a half being filled' guarantee inside the core rather than in firmware unsafe, and the whole static can be link_section-placed into AXI SRAM by .02. Unsafe is confined to the two buffer accessors plus the Sync impl, each with a SAFETY argument.
- AC #3 deviations, deliberate: claim_fill() returns a FillClaim (half(), source() byte range, buffer() -> &mut [u8] sized to the range) and FillClaim::finish() takes no valid_bytes - the claim already fixed the length, and a second source of truth could only disagree. take_block returns Played / Underrun / Idle; Idle is the not-running or pass-complete case (silence, no underrun counted).
- Pass lifecycle: const new() is idle (so it can be a static), start(pcm_bytes) arms a pass (refuses while running or on odd length; 0 completes at once with CRC16_INITIAL), finish() returns Some(PassReport{crc16, samples, underruns}) once complete and stays readable until the next start. Underrun: silence, saturating count, no advance.
- Tests: tests/replay_core.rs (11 tests) - exhaustive 4x4 transitions, byte round-trip, CRC == frame::crc16_ccitt for 1 sample, short block, one block, block+1, one half, half+1, two halves, 3 halves+3 samples, 480 000 bytes (5 s); zero length; completed-pass silence; final-claim range; late refill (underrun before fill, underrun on a poisoned Filling half, underrun on unfilled half 1, no advance, CRC still equal, is_clean false); restart; proptest over Claim/Publish/Callback sequences with poison in every Filling half; a two-thread stress test. Mutation check: letting take_block read a non-Ready half fails 6 of 11 tests including the proptest.
- Verified: cargo test -p asperitas-logging, cargo build -p asperitas-logging --target thumbv7em-none-eabihf (const asserts evaluate in 32-bit usize), RUSTDOCFLAGS=-D warnings cargo doc, scripts/gates.sh commit (15 gates green). No firmware wiring (TASK-038.04.03.02). The QSPI throughput and scheduling latency are labelled ASSUMED for TASK-038.09 to replace.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added asperitas_logging::replay, the host-tested staging core for QSPI excerpt replay: an owned, cache-line-aligned two-half ping-pong (16 KiB) with a published HalfState machine, a const-asserted staging justification (85.3 ms half vs 4x an assumed 18.2 ms refill), a FillClaim refill API, a take_block callback API that outputs silence and stalls on underrun, and a PassReport carrying the CRC-16 over exactly the source samples handed to the DAC encoder. tests/replay_core.rs proves CRC equality at every boundary length, the underrun policy, and (by proptest plus a two-thread test) that the callback never reads a half being filled. Firmware wiring is TASK-038.04.03.02.
<!-- SECTION:FINAL_SUMMARY:END -->
