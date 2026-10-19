---
id: TASK-145.01
title: >-
  Spike: measure what cpal duplex audio on macOS gives asperitas-live, and
  decide how DSP blocks relate to host buffers
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 22:07'
updated_date: '2026-10-19 22:30'
labels:
  - m4
  - live
  - planned
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/development.md
  - flake.nix
parent_task_id: TASK-145
priority: high
type: spike
ordinal: 280100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The `asperitas-live` crate (sibling ticket TASK-145.03) cannot be designed until we know what `cpal` does on this Mac. Four facts are unknown and expensive to guess: which sample rates and buffer sizes the default input and output devices accept; whether input and output share a clock (one duplex callback) or come as two streams that need a ring buffer between them; the real round-trip latency; and how the device's fixed 32-frame DSP block (`BLOCK_LENGTH` in `firmware/src/audio.rs`; firmware `main` calls `set_controls` once per block, then `process_block` on 32 frames) maps onto whatever buffer size CoreAudio hands the callback.

The design question to settle: must the DSP block size be decoupled from the host buffer size, and if so how, without changing the effect's output? Starting hypothesis to confirm or refute: `Asperitas` is per-sample (`process_block` is bit-identical to repeated `tick`, pinned by `matches_manual_composition_bit_for_bit` in `crates/asperitas-dsp/src/asperitas.rs`), so the only block-dependent behaviour is when `set_controls` is applied. The host can therefore process any buffer length with no added latency, latching a pending control change only at frame indices that are multiples of 32 counted from stream start, which reproduces the device exactly. The alternative (re-chunking into 32-frame blocks through a FIFO) adds up to 31 frames of latency.

The spike code is throwaway: do not add it to the root workspace (members = `crates/*` would pull it into every gate). Build it outside the repo, e.g. a scratch `cargo new` project in a temp directory. The deliverable is the findings, committed as a doc.

Owner decisions already made (do not reopen): crossterm-only terminal view for knobs; 48 kHz is the effect's rate (doc-001 assumptions).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 docs/investigations/cpal-macos-duplex.md exists and records, from a program actually run on this Mac (compiling is not evidence): the cpal version used, the default input and output device names, their supported sample-rate ranges, channel counts, sample formats and the buffer-size range each reports
- [x] #2 The doc records whether a 48 kHz f32 input stream and output stream both open with a fixed buffer size (try 32, 64, 128, 256, 512 frames) and the buffer size each callback actually received
- [x] #3 The doc records whether input and output callbacks share a clock (compare callback counts and frame totals over at least 30 s) and therefore whether asperitas-live needs a ring buffer between two streams, with the recommended ring capacity
- [x] #4 The doc records round-trip latency: at minimum the cpal-reported latency (input capture timestamp to output playback timestamp) per buffer size; plus a measured acoustic or cable loopback figure if a loopback is available, otherwise it says plainly that no physical loopback was measured
- [x] #5 The doc states the decision on DSP block versus host buffer (decoupled or not, and the exact mechanism) with the reason, and the same decision is recorded in this ticket's implementation notes
- [x] #6 If macOS microphone permission blocks input from the terminal, the doc says so, records what output-only facts were gathered, and the ticket notes name the exact permission step the owner must take; the ticket is not closed claiming input facts it did not observe
- [x] #7 No spike code is committed under crates/ or firmware/; bash scripts/gates.sh commit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 4316c31. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
DECISION: decouple DSP block from host buffer, no re-chunking, no added latency. asperitas-live processes whatever length the cpal callback gives in one call and latches a pending set_controls only at frame indices that are multiples of 32 counted from stream start (splitting the callback there). Verified: 200 trials, 5 s noise, random host buffers 1..1024, bit-identical to the device's 32-frame-block path. Measured on this Mac (cpal 0.18.2, built-in jack pair): 48 kHz f32 opens for fixed 32/64/128/256/512 and callbacks receive exactly the requested length; Default is 512; buffer range 15..4096; input mono, output stereo. Two separate streams, rate-locked over 35 s with constant ring fill and no underruns; ring still required. Reported path latency = 2*buffer + 274 frames. Mic permission did not block input (non-zero samples). No physical loopback was measured. Details in docs/investigations/cpal-macos-duplex.md. Spike code lived outside the repo.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Ran a scratch cpal 0.18.2 program on this Mac and recorded device configs, fixed-buffer behaviour, clock/ring behaviour, cpal-reported latency and a bit-exact block-latching check in docs/investigations/cpal-macos-duplex.md. Decision: process arbitrary host buffers directly and latch set_controls at 32-frame multiples from stream start. No physical loopback measured. gates.sh commit passes; no spike code committed.
<!-- SECTION:FINAL_SUMMARY:END -->
