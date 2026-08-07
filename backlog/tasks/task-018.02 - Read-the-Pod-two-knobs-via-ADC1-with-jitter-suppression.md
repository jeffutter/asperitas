---
id: TASK-018.02
title: Read the Pod two knobs via ADC1 with jitter suppression
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-05 17:26'
updated_date: '2026-08-07 23:42'
labels:
  - planned
dependencies:
  - TASK-018.01
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-018
priority: high
type: feature
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make both Pod potentiometers readable as normalised values, ready for TASK-019 to map onto DSP parameters.

Knob 1 is D21 = PC4; Knob 2 is D15 = PC0. Both are ADC1-capable on the STM32H750 (PC4 = ADC12_INP4, PC0 = ADC123_INP10), so a single `Adc<ADC1>` covers both — verify the channel impls compile rather than assuming. `p.ADC1` survives `new_daisy_board!` because the macro only partially moves `p`.

Pot jitter is called out in doc-001 section 7 as a Medium risk specifically because it presents as a DSP bug. embassy-stm32 0.6 ADC v4 has hardware averaging built in — `AdcConfig::averaging` accepts up to `Averaging::Samples1024`. Prefer that to a hand-rolled filter: it costs nothing at runtime and needs no state. Layer additional smoothing on top only if measurement shows it is needed, and say what the measurement was.

Report position as a normalised f32 in the inclusive range 0.0 to 1.0.

Curve shaping is explicitly NOT this ticket. A log or exponential taper for frequency-like parameters is parameter-mapping policy, and doc-001 section 3 puts that with the caller: the DSP owns parameter semantics, the host owns the knob-to-parameter mapping, and the BSP owns neither. It reports where the knob is. The taper belongs in the shared mapping that TASK-019.01 builds, so that the CLI, the Pod, and the M4 TUI all get the same curve instead of three subtly different ones.

Sample at a rate the control surface needs — roughly 1 kHz is ample for a hand-turned knob — and keep it off the audio callback path entirely. The callback runs every 32 samples (0.67 ms) and has no budget for ADC conversions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both knobs read as a normalised f32 in the inclusive range 0.0 to 1.0, monotonically increasing with physical rotation in one direction
- [ ] #2 Knob sampling happens outside the audio callback
- [ ] #3 ADC hardware averaging is configured, and the chosen sample count is recorded with the reasoning behind it
- [ ] #4 The jitter-suppression approach is stated in the implementation notes; measuring the residual on real hardware is TASK-018.04
- [ ] #5 A degenerate or out-of-range raw ADC reading cannot produce a value outside 0.0 to 1.0, and cannot panic
- [ ] #6 Builds for thumbv7em-none-eabihf, and root cargo test and clippy with -D warnings stay green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Implementation Plan: Read Pod knobs via ADC1 with jitter suppression
================================================

### Scope

Add knob reading to `crates/asperitas-pod` as a new feature-gated module (`knob.rs`). Both knobs share a single `Adc<ADC1>` instance. Normalised f32 output [0.0, 1.0]. Hardware averaging for jitter suppression. Sampling outside audio callback (by design — the API is polled, not async/DMA).

### Files

**New file: `crates/asperitas-pod/src/knob.rs`**

```rust
//! Knob (potentiometer) driver for the Daisy Pod.
//!
//! Reads both Pod potentiometers via ADC1 and reports normalised f32 values
//! in [0.0, 1.0]. Knob 1 = PC4 (ADC12_INP4), Knob 2 = PC0 (ADC123_INP10).
//!
//! ## Jitter suppression
//!
//! embassy-stm32 v4 ADC supports hardware averaging via AdcConfig::averaging.
//! We use Averaging::Samples16 — gives ~4x noise reduction (6 dB SNR improvement)
//! at minimal latency (~16 conversions × ~6.6 µs = ~105 µs per read, well within
//! a 1 kHz polling budget). See module comment for full reasoning.
//!
//! ## Normalisation
//!
//! Raw ADC value (u16, 12-bit range [0, 4095]) is divided by 4095.0 and clamped
//! to [0.0, 1.0]. Degenerate readings cannot escape bounds or panic.
//!
//! ## Curve shaping
//!
//! NOT included here. Log/exponential taper belongs in TASK-019 parameter mapping.
//! This BSP reports physical position only.

use embassy_stm32::{
    self as hal,
    adc::{Adc, AdcConfig, Averaging, SampleTime},
    Peri,
};

/// Default sample time for potentiometer inputs.
///
/// Cycles325 provides adequate settling time for high-impedance pot sources.
/// Matches embassy-stm32 H7 ADC example (examples/stm32h7/src/bin/adc.rs).
const POT_SAMPLE_TIME: SampleTime = SampleTime::Cycles325;

/// Maximum raw value for 12-bit ADC (2^12 - 1).
const ADC_12BIT_MAX: f32 = 4095.0;

/// Single knob reader backed by an ADC channel.
pub struct Knob {
    /// Which knob this is (for logging/debugging).
    id: u8,
}

impl Knob {
    /// Create a knob reader from an ADC peripheral and pin.
    ///
    /// Configures hardware averaging (Samples16) for jitter suppression.
    pub fn new(
        _id: u8,
        adc: impl Into<Peri<_, hal::peripherals::ADC1>>,
        _pin: impl Into<embassy_stm32::gpio::AnyPin>,
    ) -> Self {
        // ... construct Adc with config, store channel reference
        Self { id: _id }
    }

    /// Read the current knob position as a normalised value in [0.0, 1.0].
    ///
    /// Blocks for the duration of the ADC conversion (~105 µs with Samples16).
    /// Safe to call from any non-audio-callback context at up to ~1 kHz rate.
    pub fn read(&mut self) -> f32 {
        let raw: u16 = /* blocking_read on the channel */;
        let value = (raw as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
        value
    }
}

/// Dual-knob controller — owns the ADC1 instance and both channels.
pub struct Knobs {
    adc: Adc<hal::peripherals::ADC1>,
    knob1: KnobChannel,
    knob2: KnobChannel,
}

impl Knobs {
    pub fn new(
        adc: Peri<_, hal::peripherals::ADC1>,
        knob1_pin: Peri<_, hal::peripherals::PC4>,
        knob2_pin: Peri<_, hal::peripherals::PC0>,
    ) -> Self {
        let config = AdcConfig::default().with_averaging(Averaging::Samples16);
        let adc = Adc::new_with_config(adc, config);
        // Initialize both channels...
        Self { adc, knob1: ..., knob2: ... }
    }

    /// Read both knobs simultaneously.
    pub fn read(&mut self) -> (f32, f32) {
        let k1 = self.knob1.read(&self.adc);
        let k2 = self.knob2.read(&self.adc);
        (k1, k2)
    }
}
```

### Key design decisions

1. **Averaging::Samples16** — chosen starting point. Rationale: 16× averaging gives ~4× noise reduction (√16 = 4) per ST AN2834, at ~105 µs latency (16 × 6.6 µs conversion). Well within 1 ms budget for 1 kHz polling. If residual jitter is visible on hardware (TASK-018.04), bump to Samples32 or add EMA filter. Documented in module comment.

2. **blocking_read per channel** — simpler than DMA/buffered approach. At 1 kHz polling, sequential reads of two channels take ~210 µs total, leaving ample margin. Per-channel reprogramming overhead is negligible. No allocation, no state machine.

3. **Normalisation: `val as f32 / 4095.0_f32` + clamp** — saturating division prevents NaN/overflow. Clamp guarantees [0.0, 1.0] even if ADC returns degenerate values. No panic possible.

4. **API shape: `Knobs` struct owns ADC1 + both channels** — consumer calls `knobs.read()` to get `(f32, f32)`. Mirrors how the firmware will use it: one poll per control loop iteration, two values out.

### Integration points

- `lib.rs`: Add `#[cfg(feature = "pod-hw")] pub mod knob;`
- No changes to firmware main.rs yet — TASK-019 wires knobs into DSP parameters
- `Cargo.toml`: No new dependencies — `embassy-stm32` already provides ADC

### Verification steps

1. Host build: `cargo build -p asperitas-pod` (no features) — compiles to nothing
2. Target build: `cargo build -p asperitas-pod --features pod-hw --target thumbv7em-none-eabihf`
3. Root `cargo test` passes (no tests to break — greenfield module)
4. Root `cargo clippy -D warnings` passes
5. Module comment documents averaging choice (Samples16) and reasoning (AC #3)
6. Implementation notes document jitter-suppression approach (AC #4)

### Acceptance criteria mapping

- AC #1 (normalised f32 [0.0, 1.0]): `read()` returns `val as f32 / 4095.0_f32` clamped to [0.0, 1.0]
- AC #2 (outside audio callback): Polled API — caller decides when to read. Not async, not in callback.
- AC #3 (hardware averaging configured + documented): `AdcConfig::default().with_averaging(Averaging::Samples16)` + module comment with reasoning
- AC #4 (jitter approach documented): Module comment states hardware averaging first, EMA fallback if measurement shows need
- AC #5 (degenerate readings safe): Saturating cast + clamp prevents out-of-range and panic
- AC #6 (builds + clippy): Verified in verification steps
<!-- SECTION:PLAN:END -->
