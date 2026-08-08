---
id: TASK-018.03
title: Decode the Pod encoder and buttons with debounce
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-08 01:45'
labels:
  - planned
dependencies:
  - TASK-018.01
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make the Pod rotary encoder, its click switch, and both pushbuttons usable as decoded events.

Pins, from the table in docs/reference/daisy-pod.md: encoder A = D26 = PD11, encoder B = D25 = PA0, encoder click = D13 = PB6, Button 1 = D27 = PG9, Button 2 = D28 = PA2.

These are SAI2 pins in the Seed pinout, which is harmless here: Seed3 audio runs on SAI1 (docs/reference/daisy-seed3.md). Noting it so the overlap is not mistaken for a conflict mid-implementation.

The `exti` feature is already enabled in firmware/Cargo.toml, so `ExtiInput` is available. Polling is also legitimate, and may be simpler given TASK-018.02 already polls the knobs — one control-surface task servicing everything is easier to reason about than a mix of interrupt and polled paths. Pick one, apply it to all five inputs, and record why. Mixing the two approaches across inputs is the outcome to avoid.

Deliver decoded results, not raw pin state:
- Relative encoder movement as signed detent increments, clockwise positive.
- Press and release edges for the click and both buttons.
- Debounce, so one physical actuation yields exactly one event.

Debounce is not optional polish. The Pod uses mechanical switches and a mechanical encoder; contact bounce on an undebounced quadrature decoder reads as a jumpy, unpredictable parameter — precisely the class of symptom that gets misattributed to DSP, which doc-001 section 7 already flags for gain staging.

Gestures are out of scope. Long-press, double-click, and press-and-hold combinations are deferred: doc-001 sections 7 and 8 defer control-surface decisions until the effect reveals which controls it actually wants. Report edges and let callers compose gestures later.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Encoder rotation yields signed detent increments, with clockwise positive
- [x] #2 The encoder click and both buttons report press and release edges distinctly
- [x] #3 One physical press or one detent produces exactly one event — contact bounce does not double-count
- [x] #4 The debounce interval and the interrupt-versus-poll choice are recorded with the reasoning behind each
- [x] #5 All five digital inputs use the same approach — the implementation does not mix interrupt-driven and polled inputs
- [x] #6 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Implementation Plan: Decode Pod encoder and buttons with uniform polling
=============================================================================

## Architecture Decision: Uniform Polling

All five digital inputs (encoder A/B, encoder click, button 1, button 2) use
the same polling approach. No EXTI interrupts are mixed in. Rationale:
- AC #5 requires uniform approach across all five inputs
- Software quadrature decoding cannot be interrupt-driven cleanly (needs cross-edge state tracking)
- TASK-018.02 already polls knobs; one control-surface task servicing everything is simpler
- PD11 (encoder A) has no timer alternate function, ruling out hardware QEI anyway

Debounce interval: 5 ms consecutive stable readings for buttons/click, inherent
Gray-code LUT self-debouncing for encoder rotation.

## Module Structure

New file: 

### Components

1. **Software Quadrature Decoder** ( struct)
   - Holds previous 2-bit state and accumulated delta
   - Uses 16-entry Gray-code transition table (const array)
   -  applies LUT lookup, returns signed detent increment
   - Bounce states (both bits changing simultaneously) map to 0 in the LUT → self-debouncing

2. **Button Debouncer** ( struct)  
   - Tracks last confirmed level + consecutive stable count
   -  updates state machine
   - Emits Edge::Press or Edge::Release only after DEBOUNCE_TICKS consecutive stable readings
   - Configurable DEBOUNCE_TICKS constant (default 5 at 1 kHz = ~5 ms)

3. **ControlSurface** struct — unified driver combining all five inputs
   - Takes ownership of all five  pins at construction
   - Converts each to  (Pod switches connect pin to ground when pressed)
   -  samples all five pins in one call, returns decoded events
   - Returns an enum with variants: EncoderDelta(i8), Button1Press, Button1Release, Button2Press, Button2Release, ClickPress, ClickRelease

## API Design

## Integration Points

- : add 
- : Pin types already exist (PodSw1, PodSw2, PodEncA, PodEncB, PodEncSw)
- Firmware will construct  and poll it from an embassy task at ~1 kHz
  (same task that polls knobs, or a dedicated control-surface task)

## Test Strategy

Host-testable unit tests (no hardware feature needed):
- EncoderDecoder: verify all 16 LUT entries produce correct transitions
- EncoderDecoder: bounce states (00→11, 01→10) yield delta=0
- DebouncedSwitch: single bounce pulse does not emit edge
- DebouncedSwitch: stable press for N ticks emits exactly one Press edge
- DebouncedSwitch: release after stable period emits exactly one Release edge

These mirror the knob.rs test pattern — algorithmic logic tested on host, hardware
verification deferred to TASK-018.04.

## Files Changed

1.  — NEW: decoder, debouncer, ControlSurface
2.  — ADD: encoder module export
3.  — NO CHANGE (embassy-stm32 already optional dep)

## Verification Steps

1. Root 
running 4 tests
test wav_io::tests::read_wav_widens_mono_to_stereo ... ok
test wav_io::tests::validate_spec_rejects_wrong_bit_depth ... ok
test wav_io::tests::validate_spec_rejects_float_format ... ok
test wav_io::tests::validate_spec_rejects_more_than_two_channels ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 6 tests
test process::tests::parse_param_pairs_rejects_malformed_arg ... ok
test wav_io::tests::read_wav_widens_mono_to_stereo ... ok
test wav_io::tests::validate_spec_rejects_float_format ... ok
test wav_io::tests::validate_spec_rejects_more_than_two_channels ... ok
test wav_io::tests::validate_spec_rejects_wrong_bit_depth ... ok
test process::tests::run_process_writes_valid_stereo_from_mono_input ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 22 tests
test golden_gain_pluck ... ok
test golden_filter_sweep ... ok
test golden_gain_sweep ... ok
test golden_filter_impulse ... ok
test golden_gain_impulse ... ok
test golden_filter_pluck ... ok
test golden_gain_mandolin_chord ... ok
test golden_gain_mandolin_single_note_soft ... ok
test golden_gain_octave_mandolin_single_note_hard ... ok
test golden_filter_octave_mandolin_chord ... ok
test golden_filter_mandolin_fast_run ... ok
test golden_filter_octave_mandolin_fast_run ... ok
test golden_gain_octave_mandolin_fast_run ... ok
test golden_filter_octave_mandolin_single_note_hard ... ok
test golden_gain_mandolin_fast_run ... ok
test golden_gain_octave_mandolin_chord ... ok
test golden_filter_mandolin_single_note_hard ... ok
test golden_filter_octave_mandolin_single_note_soft ... ok
test golden_gain_octave_mandolin_single_note_soft ... ok
test golden_filter_mandolin_chord ... ok
test golden_filter_mandolin_single_note_soft ... ok
test golden_gain_mandolin_single_note_hard ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

running 16 tests
test filter::normalised_tests::params_from_normalised_clamps_above_one ... ok
test filter::normalised_tests::params_from_normalised_max_is_20khz ... ok
test filter::normalised_tests::params_from_normalised_clamps_below_zero ... ok
test filter::normalised_tests::params_from_normalised_midpoint_is_geometric_mean ... ok
test filter::normalised_tests::params_from_normalised_min_is_20hz ... ok
test filter::normalised_tests::params_from_normalised_monotonic ... ok
test gain::normalised_tests::params_from_normalised_clamps_above_one ... ok
test filter::tests::filter_parse_rejects_unknown_key ... ok
test filter::tests::filter_parse_rejects_non_numeric_value ... ok
test gain::normalised_tests::params_from_normalised_clamps_below_zero ... ok
test gain::normalised_tests::params_from_normalised_max_is_plus_12db ... ok
test gain::normalised_tests::params_from_normalised_midpoint_is_minus_24db ... ok
test gain::normalised_tests::params_from_normalised_min_is_minus_60db ... ok
test gain::normalised_tests::params_from_normalised_monotonic ... ok
test gain::tests::gain_parse_rejects_non_numeric_value ... ok
test gain::tests::gain_parse_rejects_unknown_key ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 12 tests
test filter_param_change_smooth ... ok
test gain_silence_in_silence_out ... ok
test gain_param_change_smooth ... ok
test filter_silence_in_silence_out ... ok
test gain_output_always_finite ... ok
test gain_reset_idempotent ... ok
test gain_output_bounded ... ok
test filter_output_always_finite ... ok
test filter_output_bounded ... ok
test filter_block_equals_tick ... ok
test filter_reset_idempotent ... ok
test gain_block_equals_tick ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s

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

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s passes (host-side tests for decoder logic)
2. Root  passes (no warnings)
3. Firmware compiles for thumbv7em-none-eabihf (type-checks with embassy-stm32 types)
4. Design decision documented: why polling over EXTI, why 5ms debounce, why Gray-code LUT
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixup applied post-review (commit 2703d73, fixup! of 3fba24e): EncoderDecoder::update(current_state: u8) indexed the 16-entry Gray-code LUT with an unmasked input — a caller passing current_state > 3 (the function is pub) would hit an out-of-bounds array index panic. Masked input with '& 0b11' so any u8 is in range; added regression test out_of_range_state_is_masked_not_indexed_out_of_bounds. No behavior change for the only real caller (ControlSurface::poll, which always passes 0-3).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented software quadrature decoder (Gray-code LUT with inherent self-debouncing) and debounced switch driver for all five Pod digital inputs. Uniform polling approach across encoder A/B, encoder click, button 1, and button 2. Debounce interval: 5 ms at 1 kHz poll rate. 16 host-testable unit tests verify LUT correctness, symmetry, bounce filtering, and edge emission without double-counting. Builds for thumbv7em-none-eabihf; clippy clean.
<!-- SECTION:FINAL_SUMMARY:END -->
