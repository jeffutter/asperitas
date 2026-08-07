//! Knob (potentiometer) driver for the Daisy Pod.
//!
//! Reads both Pod potentiometers via ADC1 and reports normalised f32 values
//! in [0.0, 1.0]. Knob 1 = PC4 (ADC1_INP4), Knob 2 = PC0 (ADC1_INP10).
//!
//! ## Jitter suppression
//!
//! embassy-stm32 v4 ADC supports hardware averaging via `AdcConfig::averaging`.
//! We use `Averaging::Samples16`, which gives ~4x noise reduction (√16 = 4)
//! per ST AN2834, at ~105 µs latency (16 × ~6.6 µs conversion). This is well
//! within a 1 ms budget for 1 kHz polling. If residual jitter is visible on
//! hardware (TASK-018.04), the sample count may be increased to Samples32 or
//! an EMA filter layered on top.
//!
//! ## Normalisation
//!
//! Raw ADC value (u16, 12-bit range [0, 4095]) is divided by 4095.0 and clamped
//! to [0.0, 1.0]. Degenerate readings cannot escape bounds or panic.
//!
//! ## Curve shaping
//!
//! NOT included here. Log/exponential taper belongs in TASK-019 parameter
//! mapping. This BSP reports physical position only — the DSP owns parameter
//! semantics, the host owns the knob-to-parameter mapping, and the BSP owns
//! neither.

use embassy_stm32::{
    self as hal,
    adc::{Adc, AdcConfig, Averaging, SampleTime},
};

/// Default sample time for potentiometer inputs.
///
/// CYCLES387_5 (387.5 ADC clock cycles) provides adequate settling time for
/// high-impedance pot sources on STM32H7. Per RM0468, this is sufficient for
/// external analog signals.
const POT_SAMPLE_TIME: SampleTime = SampleTime::CYCLES387_5;

/// Maximum raw value for 12-bit ADC (2^12 - 1).
const ADC_12BIT_MAX: f32 = 4095.0;

/// Dual-knob controller — owns the ADC1 instance and both channels.
///
/// Consumes ownership of the ADC peripheral and both knob pins at construction
/// time. Callers poll `read()` from a control-surface task at ~1 kHz, never
/// from the audio callback.
pub struct Knobs {
    adc: Adc<'static, hal::peripherals::ADC1>,
    knob1_pin: hal::Peri<'static, hal::peripherals::PC4>,
    knob2_pin: hal::Peri<'static, hal::peripherals::PC0>,
}

impl Knobs {
    /// Create a new knobs driver from the ADC1 peripheral and both knob pins.
    ///
    /// Configures hardware averaging (`Samples16`) for jitter suppression.
    pub fn new(
        adc: impl Into<hal::Peri<'static, hal::peripherals::ADC1>>,
        knob1_pin: impl Into<hal::Peri<'static, hal::peripherals::PC4>>,
        knob2_pin: impl Into<hal::Peri<'static, hal::peripherals::PC0>>,
    ) -> Self {
        let adc = Adc::new_with_config(
            adc.into(),
            AdcConfig {
                resolution: None,
                averaging: Some(Averaging::Samples16),
            },
        );

        Self {
            adc,
            knob1_pin: knob1_pin.into(),
            knob2_pin: knob2_pin.into(),
        }
    }

    /// Read both knobs simultaneously.
    ///
    /// Returns `(knob1, knob2)` as normalised f32 values in [0.0, 1.0],
    /// monotonically increasing with physical rotation in one direction.
    ///
    /// Each read blocks for the duration of the ADC conversion (~105 µs with
    /// Samples16). Sequential reads of two channels take ~210 µs total, well
    /// within a 1 kHz polling budget.
    ///
    /// Safe to call from any non-audio-callback context.
    pub fn read(&mut self) -> (f32, f32) {
        let raw1 = self.adc.blocking_read(&mut self.knob1_pin, POT_SAMPLE_TIME);
        let raw2 = self.adc.blocking_read(&mut self.knob2_pin, POT_SAMPLE_TIME);
        (
            (raw1 as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0),
            (raw2 as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_full_range() {
        // Full-scale reading maps to 1.0
        let raw = 4095u16;
        let value = (raw as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
        assert!((value - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_zero() {
        // Zero reading maps to 0.0
        let raw = 0u16;
        let value = (raw as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
        assert!((value - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_midpoint() {
        // Midpoint maps to ~0.5
        let raw = 2048u16;
        let value = (raw as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
        assert!((value - 0.5).abs() < 0.01);
    }

    #[test]
    fn clamp_over_range() {
        // Degenerate over-range reading is clamped to 1.0
        let raw = u16::MAX; // 65535 — impossible with 12-bit ADC but safe to handle
        let value = (raw as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
        assert_eq!(value, 1.0);
    }

    #[test]
    fn monotonic_increase() {
        // Values increase monotonically with raw ADC reading
        let mut prev = 0.0_f32;
        for i in 0..=4095u16 {
            let value = (i as f32 / ADC_12BIT_MAX).clamp(0.0, 1.0);
            assert!(value >= prev, "not monotonic at raw={}", i);
            prev = value;
        }
    }
}
