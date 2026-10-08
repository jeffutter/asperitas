---
id: TASK-038.04.03
title: >-
  Replay excerpts from QSPI through an internal-RAM ping-pong and report the
  DAC-path CRC
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-08 15:54'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03.01
  - TASK-038.04.03.02
parent_task_id: TASK-038.04
priority: high
ordinal: 133800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: rig.rs. A stimulus source that plays a slot: an internal-RAM ping-pong pair refilled by read_async over MDMA so the audio callback never touches QSPI. Justify staging depth numerically in a comment (refill latency from measured or datasheet QSPI throughput versus buffer duration at 96,000 B/s; kernel clock is unstated, so mark the estimate). Callback headroom is about 590 us of 666 us and the SDRAM window is Device memory, so keep staging in AXI SRAM or DTCM and avoid unaligned access. After one full pass report the CRC-16 of the samples handed to the DAC encoder, with code and docs stating this says nothing about what returns through codec and cable (TASK-035 AC #4 posture). Handle underrun explicitly, never silently. Acceptance: rig builds, CI covered, host tests for the staging and CRC accounting logic. Covers parent AC #4 and AC #5.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Approach: split the replay into a pure, host-testable staging core and a thin hardware integration. rig.rs renders through a build-time generator type inside the SAI1 callback (generator.process_block then encode_block), so replay becomes one more generator selected by cargo feature, but its sample source is a ping-pong rather than arithmetic. Everything that can be wrong without a board - half-buffer state machine, underrun policy, tail handling, DAC-path CRC - lives in a no_std module and is proven on the host. The device side then only moves bytes: a thread-mode task calls Flash::read_async (MDMA, QUADSPI+MDMA interrupts bound per examples/flash.rs) to refill the idle half; the callback pops and never touches QSPI.

Sub-tickets: .01 staging core (no deps). .02 rig wiring and EXCPLAY record (after .01 and TASK-038.04.05, which owns the Flash build and interrupt bindings and the slot header read).
Order: .01, then .02.

Design constraints found: the sample CRC must be taken over the s16 values handed to encode_block, before f32 conversion round-trips, so the host-side expected value is crc16_ccitt of the WAV data chunk (slot header from TASK-038.04.01 stores the same CRC). Staging must sit in AXI SRAM/D2, not SDRAM (Device memory, no unaligned access) and DMA into cacheable memory needs invalidate or a non-cacheable placement; decide and document in .02. Sample rate 48 kHz mono, 96,000 B/s; the stereo output duplicates mono. Callback headroom is about 590 us of 666 us, so the pop path must be a few instructions per block. Blocking flash API forbidden; async timeouts panic.

Verification: cargo test for the core; firmware build of rig with the new feature on thumb and the existing CI configurations; clippy. Bench verification (hear it, compare reported CRC to the host CRC, observed margin) is TASK-038.09 (@human).
Remaining work not covered: none; docs are TASK-038.04.06.
<!-- SECTION:PLAN:END -->
