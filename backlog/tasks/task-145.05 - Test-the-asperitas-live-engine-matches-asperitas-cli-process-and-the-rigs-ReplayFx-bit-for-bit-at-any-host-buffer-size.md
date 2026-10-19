---
id: TASK-145.05
title: >-
  Test: the asperitas-live engine matches asperitas-cli process and the rig's
  ReplayFx bit for bit at any host buffer size
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 22:10'
labels:
  - m4
  - live
  - test
dependencies:
  - TASK-145.03
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - audio/README.md
  - docs/reference/asperitas-rig.md
parent_task_id: TASK-145
priority: high
type: task
ordinal: 280500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
This is the host half of the M4 exit criterion (doc-001): the live host and the device, given the same `Controls` and input, produce the same output within float tolerance. The device half is the @human bench ticket in this umbrella, which reruns the rig excerpt comparison.

Two references, because they cover different claims:
1. Offline CLI: `asperitas_cli::process::run_process` (or the same `Asperitas::new(48_000.0, &controls).process_block` on the whole clip) at fixed `Controls`. This is what the goldens pin and what `asperitas-measure analyze --excerpt` uses as its reference for the device, so live == CLI plus CLI ~ device (bench) closes the loop.
2. The device code path with moving knobs: `asperitas_stimulus::replay_fx::ReplayFx` (or `fx_chain::FxChain`) driven in 32-frame blocks with `set_knobs` before each block, which is the no_std code the rig firmware runs, and the same per-block `set_controls` then `process_block` shape as firmware `main`'s `KnobChain::process`. The CLI cannot follow a moving knob, so this reference is the one that pins the block-grid control latch.

Tolerance: bit-identical (compare `f32::to_bits`), because host and device run the same `asperitas-dsp` code on the same input. If the implementer finds a reason output cannot be bit-identical (it should not happen on one machine), record the reason in the notes and fall back to the goldens' 1e-6 absolute, stating it in the test. The test compares the engine's pre-clamp output; the [-1, 1] clamp at the device boundary is outside the comparison and stated so.

Tests must not open an audio device: they drive the engine core TASK-145.03 built, not cpal.

Code comments describe current state only: no ticket numbers, dates, measurement history, or line numbers of other files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An integration test in crates/asperitas-live/tests/ feeds every clip in audio/instruments through the engine at three fixed Controls (Controls::default(), cutoff_hz=2000 gain_db=-6, and the two knobs at 0.2 and 0.9) and asserts bit-identical output to the offline CLI path for the same clip and Controls
- [ ] #2 The same test runs each case at host buffer sizes 1, 31, 32, 33, 64, 480, 512, 1024 and a seeded random-length sequence, and the output is identical across all of them
- [ ] #3 A second test schedules knob changes at frame offsets that are and are not multiples of 32 and asserts the engine's output is bit-identical to ReplayFx (or FxChain) driven in 32-frame blocks with the same changes applied at the next 32-frame boundary
- [ ] #4 A negative control proves the test can fail: applying the change at the exact requested frame instead of the next 32-frame boundary makes the moving-knob comparison fail (kept as a test asserting inequality)
- [ ] #5 The stated tolerance and why it is that, and that the device boundary clamp is excluded, are written in the test's module doc
- [ ] #6 Runtime is reasonable for cargo test (the gate): if the full matrix is slow in debug, the root Cargo.toml dev-profile opt-level override covers asperitas-live, as it does for asperitas-dsp
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
