---
id: TASK-019.01
title: Run a Processor in the audio callback with knobs mapped to its parameters
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-08-05 17:27'
updated_date: '2026-08-08 00:13'
labels:
  - planned
dependencies:
  - TASK-018.02
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-019
priority: high
type: feature
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the passthrough in firmware/src/bin/main.rs with an `asperitas_dsp::Processor` driven by the Pod knobs.

Use the processors that already exist rather than writing new DSP. `OnePoleLowPass` on knob 1 is the right first choice because a filter sweep is unmistakable by ear — when the goal is proving a whole signal chain, an ambiguous result is worthless. `Gain` on knob 2 gives a second, independently audible axis. The resonator is M5; do not start it here.

THE KNOB-TO-PARAMETER MAPPING MUST BE SHARED, NOT FIRMWARE-LOCAL. TASK-008.01 established the pattern: `parse_params_from_cli` lives on each processor type inside asperitas-dsp, next to its `Params` definition, so every host imports one mapping instead of inventing its own. A normalised-knob mapping belongs in the same place for the same reason. If the device and the CLI apply different curves, they stop sounding alike, and comparing them — which is how doc-001 section 7 proposes to distinguish gain-staging problems from DSP problems — stops meaning anything. This is also where the knob taper deliberately excluded from TASK-018.02 belongs.

Real-time constraints, per doc-001 section 3: no allocation and nothing fallible in the callback. Update parameters at block rate, not per sample. Smoothing already lives inside the processors (`Smoother` in asperitas-dsp), which is what makes a per-block `set_params` safe from zipper noise — do not add a second smoothing layer in the firmware.

Two facts to build against rather than assume:
- Blocks are 32 samples, not the 48 that doc-001 section 1 assumed. daisy-embassy hardcodes `BLOCK_LENGTH = 32` (src/audio.rs:13). This is known and recorded under TASK-011.
- The sample rate is 48 kHz: `AudioConfig::default()` is `Fs::Fs48000`, confirmed during TASK-005.02. Call `set_sample_rate` explicitly anyway rather than relying on a processor default, which is what doc-001 section 3 designed that method for.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The audio callback runs a Processor from asperitas-dsp instead of copying input to output
- [ ] #2 Both Pod knobs drive processor parameters through a mapping defined in asperitas-dsp and shared with asperitas-cli, not defined in firmware
- [ ] #3 asperitas-cli can be driven with the same normalised knob positions as the device and produces the same parameter values, so device and desktop output are comparable on identical source material
- [ ] #4 set_sample_rate is called with the device actual rate rather than relying on the processor default
- [ ] #5 The callback performs no heap allocation and cannot panic at any knob position, including both extremes
- [ ] #6 Parameters are updated at block rate, and no smoothing is duplicated in the firmware on top of the processor own
- [ ] #7 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Architecture overview

Replace the passthrough in firmware/src/bin/main.rs with a two-stage DSP chain:
OnePoleLowPass (knob 1) → Gain (knob 2). Knob values are read by a background
control-surface task at ~1 kHz and stored in static memory; the audio callback
reads them at block rate via critical_section. Parameter mapping lives in
asperitas-dsp next to each processor's Params definition, shared between
firmware and CLI.

### Step 1: Add params_from_normalised to asperitas-dsp

**File: crates/asperitas-dsp/src/filter.rs**
- Add inherent method on OnePoleLowPass beside parse_params_from_cli
- Logarithmic taper: map knob [0,1] → cutoff_hz [20 Hz, 20000 Hz].
  Formula: cutoff = 20.0 * (20000.0 / 20.0).powf(knob.clamp(0.0, 1.0))
  At knob=0.5 this gives geometric mean ≈ 447 Hz.
- Clamp input to [0.0, 1.0] so degenerate readings produce valid params
- Add #[cfg(test)] unit tests verifying min→20 Hz, max→≈20000 Hz, midpoint→≈447 Hz, monotonic

**File: crates/asperitas-dsp/src/gain.rs**
- Add inherent method on Gain beside parse_params_from_cli
- Linear mapping: gain_db = -60.0 + knob.clamp(0.0, 1.0) * 72.0
  Range: [-60 dB silence, +12 dB headroom]
- Add #[cfg(test)] unit tests verifying min→-60 dB, max→+12 dB, midpoint→-24 dB, monotonic

Note: These methods are NOT behind #[cfg(feature = "std")] because they take f32 input,
making them usable in no_std firmware context.

### Step 2: Add asperitas-pod dependency to firmware

**File: firmware/Cargo.toml**
- Add asperitas-pod = { path = "../crates/asperitas-pod", features = ["pod-hw"] }

### Step 3: Rewrite firmware/src/bin/main.rs

**Shared state for knobs** — follows asperitas-logging::led pattern
(static + raw pointer, safe on single-core Cortex-M).

**Separate static for latest knob readings** (two f32 values, read by callback)
using UnsafeCell + critical_section.

**main() structure changes:**

1. After new_daisy_board!(p), extract ADC1 and knob pins from board.p.ADC1,
   board.pins.knob1, board.pins.knob2. Initialize the static Knobs instance.

2. Create processors: OnePoleLowPass::default(), Gain::default().
   Call set_sample_rate(48_000.0) on both explicitly.

3. Spawn a knob-polling async task (~1 kHz interval) that reads knobs and stores
   values in shared static for callback access.

4. In start_callback, closure captures both processors. Inside callback:
   - Read latest knob positions via critical_section
   - Convert u32 input buffer → [Frame; 32] stack array (inline loop)
   - filter.set_params(&OnePoleLowPass::params_from_normalised(k1)) once per block
   - filter.process_block(&frames_in, &mut frames_out)
   - gain.set_params(&Gain::params_from_normalised(k2)) once per block
   - gain.process_block(&frames_out, &mut frames_processed)
   - Convert processed frames back to u32 output buffer (inline loop)

**Buffer conversion** (inline, no allocation):
The callback receives &[u32] / &mut [u32] of length 64 (interleaved stereo,
32 frames × 2 channels). Must convert to/from [Frame; 32] where Frame = [f32; 2].

CRITICAL: Verify the codec sample format BEFORE implementing conversion.
The TAC5242 codec may deliver 24-bit left-justified, 24-bit right-justified,
or 32-bit signed PCM in u32 containers. Check daisy-embassy's codec module
for the exact packing. The existing passthrough (output.copy_from_slice(input))
works byte-for-byte, so round-tripping through f32 must preserve samples
exactly when gain=0dB and cutoff=Nyquist.

### Step 4: Add static_cell dependency

Verify static_cell is available (check if cortex-m or embassy-stm32 brings it
transitively, or add explicitly to firmware/Cargo.toml).

### Key design decisions

1. **No sub-tickets.** All changes are tightly coupled — DSP mapping and
   firmware integration must ship together.

2. **Two processors in series.** OnePoleLowPass then Gain. The order matters:
   filtering before gain avoids amplifying noise above the cutoff. Standard
   signal chain order.

3. **Mapping ranges chosen for audibility.** 20 Hz–20 kHz covers full audible
   spectrum; -60 dB to +12 dB gives practical silence through comfortable
   headroom. Adjustable later based on hardware testing (TASK-019.02).

4. **Logarithmic taper for frequency.** Human hearing perceives pitch
   logarithmically; equal physical knob travel should give equal perceptual
   steps. Linear taper would cluster useful frequencies at one end.

### Verification checklist

- cargo test -p asperitas-dsp — new unit tests for params_from_normalised
- cargo build -p asperitas-firmware --target thumbv7em-none-eabihf — builds
- cargo clippy --target thumbv7em-none-eabihf -D warnings — clean
- cargo test (workspace) — all existing tests still pass
- Callback performs zero heap allocation (stack arrays + static refs only)
- No fallible operations in callback (all paths clamp/saturate)
<!-- SECTION:PLAN:END -->
