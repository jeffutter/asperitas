---
id: TASK-145.01
title: >-
  Spike: measure what cpal duplex audio on macOS gives asperitas-live, and
  decide how DSP blocks relate to host buffers
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 22:07'
updated_date: '2026-10-19 22:07'
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
- [ ] #1 docs/investigations/cpal-macos-duplex.md exists and records, from a program actually run on this Mac (compiling is not evidence): the cpal version used, the default input and output device names, their supported sample-rate ranges, channel counts, sample formats and the buffer-size range each reports
- [ ] #2 The doc records whether a 48 kHz f32 input stream and output stream both open with a fixed buffer size (try 32, 64, 128, 256, 512 frames) and the buffer size each callback actually received
- [ ] #3 The doc records whether input and output callbacks share a clock (compare callback counts and frame totals over at least 30 s) and therefore whether asperitas-live needs a ring buffer between two streams, with the recommended ring capacity
- [ ] #4 The doc records round-trip latency: at minimum the cpal-reported latency (input capture timestamp to output playback timestamp) per buffer size; plus a measured acoustic or cable loopback figure if a loopback is available, otherwise it says plainly that no physical loopback was measured
- [ ] #5 The doc states the decision on DSP block versus host buffer (decoupled or not, and the exact mechanism) with the reason, and the same decision is recorded in this ticket's implementation notes
- [ ] #6 If macOS microphone permission blocks input from the terminal, the doc says so, records what output-only facts were gathered, and the ticket notes name the exact permission step the owner must take; the ticket is not closed claiming input facts it did not observe
- [ ] #7 No spike code is committed under crates/ or firmware/; bash scripts/gates.sh commit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Scratch project outside the repo (a temp dir, not crates/): `cargo new cpal-spike`, add `cpal` at its current 0.x release on crates.io (record the exact version from Cargo.lock in the doc). Build inside the repo's nix dev shell (`nix develop /Users/jeffutter/src/asperitas --command cargo run --release`) so the toolchain matches; on macOS cpal uses CoreAudio and needs nothing from the flake.
2. Enumerate: default host, default input and output devices, `supported_input_configs` / `supported_output_configs` (rate range, channels, sample format, `SupportedBufferSize`). Print, paste into the doc.
3. Open 48 kHz f32 input and output streams with `BufferSize::Fixed(n)` for n in 32, 64, 128, 256, 512 and `Default`. For each, log the frame count each callback receives (min/max/histogram over ~10 s) and whether the build or play call errors.
4. Clock check: run input and output together for >= 30 s at one buffer size, counting total frames per side and the drift between them. Pass input to output through a lock-free SPSC ring (e.g. `ringbuf` or a hand-rolled one in the spike) and log underruns/overruns. Recommend a ring capacity.
5. Latency: from `InputCallbackInfo::timestamp().capture` and `OutputCallbackInfo::timestamp().playback`, compute the reported path latency per buffer size plus the ring fill. If an audio interface with a cable loopback is plugged in, emit a click and time its return; if not, say no physical loopback was measured.
6. Block decision: write a 20-line check in the spike (it may depend on `asperitas-dsp` by path) that runs a 5 s noise signal through `Asperitas` two ways with one control change scheduled mid-signal: (a) 32-frame blocks with `set_controls` before each block, as firmware main does; (b) random-length host buffers with the change latched at the next frame index divisible by 32. Compare bit for bit. Record the result and the decision.
7. Write docs/investigations/cpal-macos-duplex.md (facts, tables, the decision; no ticket numbers needed in prose beyond a header line linking the ticket). Record the decision in the ticket notes. If mic permission blocks input (macOS TCC prompt for the terminal app, or silent all-zero input), record it as in AC #6.
8. `bash scripts/gates.sh commit`, commit the doc only.
<!-- SECTION:PLAN:END -->
