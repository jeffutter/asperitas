---
id: TASK-007
title: 'asperitas-dsp: Processor trait, trivial processor, property tests'
status: Done
assignee:
  - '@agent'
created_date: '2026-08-01 05:46'
updated_date: '2026-08-01 16:28'
labels:
  - planned
dependencies:
  - TASK-002
priority: high
type: feature
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Establish the interface every other part of the project is organised around, and the test harness the real DSP work will lean on. Deliberately paired with a trivial processor (gain and a one-pole filter) — the point is the boundary and the tests, not the algorithm.

Sketch:
```rust
pub type Frame = [f32; 2];

pub trait Processor {
    type Params: Clone + Default;
    fn set_sample_rate(&mut self, hz: f32);
    fn set_params(&mut self, params: &Self::Params);
    fn tick(&mut self, input: Frame) -> Frame;
    fn reset(&mut self);
    fn process_block(&mut self, input: &[Frame], output: &mut [Frame]) { /* provided */ }
}
```

Design intent, worth preserving:
- **`tick` is the primitive, `process_block` is provided.** Callers who want blocks get them free; implementers write the simple thing. Inverting this forces every processor to reimplement buffering.
- **Params are an associated type, not a bag of floats.** Knob-to-parameter *mapping* is policy belonging to the caller (Pod, CLI, TUI); the DSP owns parameter *semantics*.
- **No allocation, no `Result`.** A real-time path that can fail or block is a design error. Clamp, saturate, and make degenerate parameter values ordinary ones rather than erroring.
- **Parameter smoothing lives inside processors**, so all four hosts don't reimplement it.
- **`set_sample_rate` separate from construction**, so one instance works at the device's 48 kHz and at whatever rate a WAV file happens to be.

Build on `dasp` primitives (ring buffers, interpolation, frame/sample traits) — from scratch, per the DSP-learning goal, rather than adopting a graph library.

Property tests with `proptest`, written as a reusable suite any `Processor` implementation can be run through, since every later processor needs the same guarantees:
- output always finite — no NaN, no infinity, for any parameter combination
- output bounded; no runaway feedback under any legal parameter set
- silence in, silence out once any tail has decayed
- `reset()` idempotent; a reset processor given identical input produces identical output
- `process_block` agrees sample-for-sample with repeated `tick`
- parameter changes produce no discontinuity above a threshold — this is the test that catches missing smoothing
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `Processor` trait defined in asperitas-dsp with the shape above
- [x] #2 A gain and a one-pole filter processor implement it
- [x] #3 The property-test suite is generic over `Processor` and reusable by future implementations
- [x] #4 All listed invariants are covered by proptest cases and pass
- [x] #5 asperitas-dsp remains `#![no_std]` with no hardware dependencies
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Overview
Replace the stub `process_sample()` with a proper `Processor` trait architecture. Two trivial processors (gain, one-pole low-pass filter) serve as reference implementations and prove the trait works. A generic proptest harness validates six invariants for any Processor. All production code stays `#![no_std]`; tests use integration-test binaries with the `std` feature and proptest dev-dependency.

**Note on dasp:** `dasp_sample 0.11.0` requires `#![feature(core_intrinsics)]` for `no_std`, which does not compile on stable Rust. The project uses stable toolchain. Therefore, we do NOT depend on dasp — we use raw `[f32; 2]` as the sketch already specifies. This is not a deficiency; dasp provides ring buffers and interpolation primitives we don't need at this layer.

### Files to create/modify

#### 1. `crates/asperitas-dsp/src/lib.rs` — rewrite
- `#![no_std]` crate root
- Re-export public items: `Frame`, `Processor` trait, modules
- Remove the old `process_sample()` stub

#### 2. `crates/asperitas-dsp/src/processor.rs` — new
- `pub type Frame = [f32; 2];`
- `Processor` trait with the exact shape from the sketch:
  - Associated type `Params: Clone + Default`
  - Required: `set_sample_rate(hz: f32)`, `set_params(&Params)`, `tick(Frame) -> Frame`, `reset()`
  - Provided: `process_block(input: &[Frame], output: &mut [Frame])` — tight for-loop calling `tick`
- Design decisions:
  - `Params` as associated type (not generic) — cleaner ergonomics, forces every impl to provide Default
  - No `Result` anywhere — clamp, saturate, degenerate → ordinary
  - `tick` is primitive, `process_block` is provided (inverts the common pattern deliberately)

#### 3. `crates/asperitas-dsp/src/smooth.rs` — new
- Internal (`pub(crate)`) parameter smoothing utilities
- `Smoother<f32>` struct: holds current value, target value, alpha coefficient
  - Alpha derived from time constant in milliseconds: `alpha = 1.0 - (-1.0 / (time_ms * sample_rate as f32)).exp()`
  - `advance(&mut self)` → returns the smoothed value
  - `set_target(&mut self, val)` updates target
  - `snap(&mut self)` sets current = target (for reset)
- Each processor creates Smoother instances per-smoothed-parameter during `set_params`

#### 4. `crates/asperitas-dsp/src/gain.rs` — new
- `GainParams { gain_db: f32 }` implementing `Clone + Default` (default: 0.0 dB = unity)
- `Gain` struct: sample rate, two `Smoother` instances (one per channel), current smoothed gain linear
- Conversion: dB → linear via `10_f32.powf(db / 20.0)`, clamped to avoid overflow
- `tick`: multiply each channel by smoothed gain, saturate to ±1.0
- `reset`: snap smoothers, clear any transient state

#### 5. `crates/asperitas-dsp/src/filter.rs` — new
- `FilterParams { cutoff_hz: f32 }` implementing `Clone + Default` (default: 20000 Hz = bypass)
- `OnePoleLowPass` struct: sample rate, smoothed coefficient, previous output sample
- Coefficient calculation: `alpha = 1.0 - (-2.0 * PI * cutoff / sample_rate).exp()` (standard one-pole)
- Edge cases: cutoff ≥ sample_rate/2 → pass-through (alpha ≈ 1.0); cutoff ≤ 0 → mute (alpha ≈ 0.0)
- `tick`: `out = prev + alpha * (in - prev)`, saturate, update prev
- Parameter smoothing applies to the coefficient (not the cutoff frequency directly) to avoid nonlinear artifacts

#### 6. `crates/asperitas-dsp/Cargo.toml` — add dev-dependencies
- Add `proptest` as dev-dependency (gated behind `std` feature)
- Keep `default = []`, `std = []` features

#### 7. `crates/asperitas-dsp/tests/property_tests.rs` — new integration test
Generic test harness using a helper function pattern:
```rust
fn run_properties<P, F>(make: F)
where
    P: Processor,
    P::Params: Clone + Default,
    F: Fn() -> P,
{
    // ... runs all six properties
}
```

Six proptest cases:

| # | Property | Implementation |
|---|----------|---------------|
| 1 | Output always finite | Generate arbitrary signal + params, assert `output.iter().all(f32::is_finite)` |
| 2 | Output bounded | Same signal, assert `sample.abs() <= 1.0 + epsilon` (epsilon accounts for smoothing overshoot) |
| 3 | Silence → silence after tail | Feed silence for 2× block size, check all zeros within 1e-6 |
| 4 | reset() idempotent | Process arbitrary signal → reset → replay same signal → bitwise compare outputs |
| 5 | process_block ≡ tick chain | For a block of N frames, compare `process_block` output with manual `tick` loop |
| 6 | No discontinuity above threshold | Constant input → change params mid-block → check `abs(out[i] - out[i-1]) < 1e-3` |

Signal generation strategy:
- Use `proptest::collection::slice` with `f32::from(-1.0..=1.0)` for amplitudes
- Include edge cases: DC (constant), max amplitude ±1.0, zero-crossing dense signals
- Block sizes: 4, 8, 16, 64, 256 (powers of 2 matching typical DMA sizes)
- Sample rates: 44100, 48000, 96000

Two concrete test functions call the harness:
- `#[test] fn gain_properties()` — runs against `Gain`
- `#[test] fn filter_properties()` — runs against `OnePoleLowPass`

### Implementation order

1. Write `smooth.rs` (no dependencies on anything else in the crate)
2. Write `processor.rs` (trait definition, depends on nothing)
3. Write `lib.rs` module declarations (wires smooth, processor, gain, filter)
4. Write `gain.rs` (depends on processor trait + smoother)
5. Write `filter.rs` (depends on processor trait + smoother)
6. Verify `cargo build -p asperitas-dsp` passes
7. Add proptest dev-dependency to Cargo.toml
8. Write `tests/property_tests.rs`
9. Run `cargo test -p asperitas-dsp --features asperitas-dsp/std`
10. Verify `cargo build --release --features seed3` still works in firmware/

### Why no sub-tickets

All deliverables are tightly coupled: the trait shape determines processor implementations, which determine test harness API. Each piece is small (<100 lines) and they must ship atomically for any of them to be verifiable. Splitting would create partial states that cannot build or test independently.

### Risks & mitigations

- **dasp unusable on stable**: Already discovered. Mitigated by using raw arrays — the ticket sketch already specified this.
- **proptest strategies for f32 may generate NaN/Inf**: The `f32::from(-1.0..=1.0)` strategy in proptest includes subnormals but not NaN/Inf by default. Explicitly include them in a separate edge-case test to verify robustness.
- **Smoothing convergence tolerance**: The discontinuity test threshold (1e-3) needs tuning. If too strict, legitimate smoothing tails fail. If too loose, unsmoothed params pass. Start conservative and adjust based on actual results.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation complete:

- processor.rs: Processor trait with Frame type, tick/process_block pattern, Params associated type
- smooth.rs: Smoother struct with time-constant-based exponential smoothing (uses libm::expf for no_std)
- gain.rs: Gain processor with per-channel smoothing, dB→linear conversion, saturation at ±1.0
- filter.rs: OnePoleLowPass with smoothed coefficient, standard one-pole IIR formula
- lib.rs: #![no_std] crate root with module declarations and re-exports
- Cargo.toml: libm 0.2 dependency, proptest 1 dev-dependency
- tests/property_tests.rs: 12 property tests (6 per processor) covering all invariants

All 12 proptest cases pass:
- gain_output_always_finite ✓
- gain_output_bounded ✓
- gain_silence_in_silence_out ✓
- gain_reset_idempotent ✓
- gain_block_equals_tick ✓
- gain_param_change_smooth ✓
- filter_output_always_finite ✓
- filter_output_bounded ✓
- filter_silence_in_silence_out ✓
- filter_reset_idempotent ✓
- filter_block_equals_tick ✓
- filter_param_change_smooth ✓

Note: libm crate used for f32::exp() in no_std context. Firmware target not available on this machine but crate builds correctly.

Fixup applied post-review (commit e894b38, git commit --fixup=29a0fd3): removed the `assert_eq!(input.len(), output.len())` panic guard from `Processor::process_block`'s default impl in crates/asperitas-dsp/src/processor.rs. It contradicted the trait's own documented design principle ("No allocation, no `Result`... a real-time path that can fail or block is a design error") three lines above it. `.zip()` already stops at the shorter slice, so mismatched-length calls now process the overlapping prefix instead of panicking, per CLAUDE.md's "Define Errors Out of Existence" guidance. Verified: `cargo test -p asperitas-dsp --features asperitas-dsp/std` (12/12 pass) and `cargo clippy -p asperitas-dsp --all-targets -- -D warnings` (clean).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented Processor trait architecture with gain and one-pole low-pass filter processors, plus a comprehensive proptest suite of 12 property tests covering all six invariants (output finite, bounded, silence decay, reset idempotency, block≡tick equivalence, smooth parameter transitions). Uses libm for no_std-compatible exp(). All tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
