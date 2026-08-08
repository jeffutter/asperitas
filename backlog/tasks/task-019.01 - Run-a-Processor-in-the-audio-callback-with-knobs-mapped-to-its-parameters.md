---
id: TASK-019.01
title: Run a Processor in the audio callback with knobs mapped to its parameters
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-05 17:27'
updated_date: '2026-08-08 00:06'
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
## Implementation Plan

### Architecture overview

Replace the passthrough in firmware/src/bin/main.rs with a two-stage DSP chain: OnePoleLowPass (knob 1) → Gain (knob 2). Knob values are read by a background control-surface task at ~1 kHz and stored in static memory; the audio callback reads them at block rate via critical_section. Parameter mapping lives in asperitas-dsp next to each processor's Params definition, shared between firmware and CLI.

### Step 1: Add params_from_normalised to asperitas-dsp

**File: crates/asperitas-dsp/src/filter.rs**
- Add inherent method  on 
- Logarithmic taper: map knob [0,1] → cutoff_hz [20 Hz, 20000 Hz] using  or equivalently 
- Clamp input to [0.0, 1.0] so degenerate readings produce valid params
- Add unit tests (#[cfg(test)]) verifying: min=20 Hz, max≈20000 Hz, midpoint≈447 Hz (geometric mean), monotonic increase across [0,1]

**File: crates/asperitas-dsp/src/gain.rs**
- Add inherent method  on 
- Linear mapping: gain_db = -60.0 + knob * 72.0 (range [-60 dB, +12 dB])
- Clamp input to [0.0, 1.0]
- Add unit tests verifying: min=-60 dB, max=+12 dB, midpoint=-24 dB, monotonic

Note: These methods are NOT behind #[cfg(feature = "std")] because they take f32 input (not String), making them usable in no_std firmware context.

### Step 2: Add asperitas-pod dependency to firmware

**File: firmware/Cargo.toml**
- Add 

### Step 3: Rewrite firmware/src/bin/main.rs

Shared state for knobs — follows asperitas-logging::led pattern (static + raw pointer):

main() structure changes:
1. After , extract ADC1 and knob pins: , , . Call  to store in static.
2. Create processors: , . Call  on both explicitly.
3. Spawn knob polling task (~1 kHz interval, calls  → stores  in another static).
4. In , capture both processors and the static knob values. Inside callback:
   - Read latest knob positions via critical_section
   - Convert u32 input buffer →  stack array (inline loop, no alloc)
   -  once per block
   - 
   -  once per block
   - 
   - Convert processed frames back to u32 output buffer (inline loop)

Buffer conversion logic (inline, no alloc):

Actually — verify the codec format first. The TAC5242 may deliver 24-bit or 32-bit samples. Check daisy-embassy codec code for the exact packing format before committing to conversion math. The existing passthrough () works byte-for-byte, so whatever format the codec delivers, round-tripping through f32 must preserve it exactly when gain=0dB and cutoff=Nyquist.

### Step 4: Add static_cell dependency

If not already present, add  to firmware/Cargo.toml. (Check if cortex-m already re-exports it or if embassy-stm32 brings it in transitively.)

### Verification checklist

- 
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 12 tests
test gain_silence_in_silence_out ... ok
test filter_param_change_smooth ... ok
test gain_param_change_smooth ... ok
test filter_silence_in_silence_out ... ok
test gain_reset_idempotent ... ok
test filter_output_bounded ... ok
test gain_output_bounded ... ok
test filter_block_equals_tick ... ok
test gain_block_equals_tick ... ok
test gain_output_always_finite ... ok
test filter_reset_idempotent ... ok
test filter_output_always_finite ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s — new unit tests pass
-  — builds cleanly
-  — clean
- 
running 4 tests
test wav_io::tests::read_wav_widens_mono_to_stereo ... ok
test wav_io::tests::validate_spec_rejects_more_than_two_channels ... ok
test wav_io::tests::validate_spec_rejects_float_format ... ok
test wav_io::tests::validate_spec_rejects_wrong_bit_depth ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 6 tests
test process::tests::parse_param_pairs_rejects_malformed_arg ... ok
test wav_io::tests::validate_spec_rejects_float_format ... ok
test process::tests::run_process_writes_valid_stereo_from_mono_input ... ok
test wav_io::tests::read_wav_widens_mono_to_stereo ... ok
test wav_io::tests::validate_spec_rejects_wrong_bit_depth ... ok
test wav_io::tests::validate_spec_rejects_more_than_two_channels ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 22 tests
test golden_gain_impulse ... ok
test golden_filter_impulse ... ok
test golden_filter_pluck ... ok
test golden_gain_pluck ... ok
test golden_filter_sweep ... ok
test golden_gain_sweep ... ok
test golden_gain_octave_mandolin_fast_run ... ok
test golden_filter_octave_mandolin_fast_run ... ok
test golden_gain_mandolin_fast_run ... ok
test golden_gain_mandolin_single_note_soft ... ok
test golden_gain_mandolin_chord ... ok
test golden_filter_mandolin_fast_run ... ok
test golden_filter_octave_mandolin_single_note_soft ... ok
test golden_filter_octave_mandolin_single_note_hard ... ok
test golden_gain_octave_mandolin_chord ... ok
test golden_filter_mandolin_single_note_hard ... ok
test golden_filter_mandolin_chord ... ok
test golden_filter_mandolin_single_note_soft ... ok
test golden_gain_octave_mandolin_single_note_soft ... ok
test golden_gain_octave_mandolin_single_note_hard ... ok
test golden_gain_mandolin_single_note_hard ... ok
test golden_filter_octave_mandolin_chord ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

running 4 tests
test filter::tests::filter_parse_rejects_non_numeric_value ... ok
test filter::tests::filter_parse_rejects_unknown_key ... ok
test gain::tests::gain_parse_rejects_non_numeric_value ... ok
test gain::tests::gain_parse_rejects_unknown_key ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 12 tests
test gain_silence_in_silence_out ... ok
test gain_param_change_smooth ... ok
test filter_param_change_smooth ... ok
test filter_silence_in_silence_out ... ok
test filter_output_always_finite ... ok
test gain_reset_idempotent ... ok
test gain_output_bounded ... ok
test filter_output_bounded ... ok
test filter_block_equals_tick ... ok
test gain_output_always_finite ... ok
test filter_reset_idempotent ... ok
test gain_block_equals_tick ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (workspace) — all existing tests still pass
- Callback performs zero heap allocation (verified by inspection — only stack arrays and static refs)
- No fallible operations in callback (clamping handles all edge cases)
<!-- SECTION:PLAN:END -->
