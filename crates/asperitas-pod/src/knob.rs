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
//! Note that `Averaging` sets the oversampling right-shift (OVSS) to match the
//! sample count, so the averaged result comes back on the same scale as a
//! single conversion. Averaging does not change the full-scale count.
//!
//! ## Normalisation
//!
//! The raw reading is divided by [`POT_FULL_SCALE_COUNTS`], which must equal
//! the full-scale count of the ADC resolution actually programmed into the
//! hardware. **These two are easy to let drift apart, and the failure is
//! silent** — see the resolution note on [`Knobs::new`]. A compile-time
//! assertion in this module now ties them together.
//!
//! ## Curve shaping
//!
//! NOT included here. Log/exponential taper belongs in TASK-019 parameter
//! mapping. This BSP reports physical position only — the DSP owns parameter
//! semantics, the host owns the knob-to-parameter mapping, and the BSP owns
//! neither.

// ---------------------------------------------------------------------------
// Normalisation — host-testable, no hardware feature needed
// ---------------------------------------------------------------------------

/// Full-scale ADC count that raw knob readings are normalised against.
///
/// The STM32H7 ADC1 runs at **16-bit** resolution (see [`Knobs::new`]), so a
/// pot at its electrical end stop reads 65535, not 4095.
pub const POT_FULL_SCALE_COUNTS: u32 = 65_535;

/// Normalise a raw ADC reading to [0.0, 1.0].
///
/// The clamp cannot trigger while [`POT_FULL_SCALE_COUNTS`] is `u16::MAX` — no
/// `u16` divided by 65535 exceeds 1.0. It is kept so that lowering the
/// resolution (and the constant with it) stays safe rather than becoming a
/// silent out-of-range bug.
pub fn normalize(raw: u16) -> f32 {
    (raw as f32 / POT_FULL_SCALE_COUNTS as f32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Hardware driver — requires embassy-stm32 (pod-hw feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "pod-hw")]
mod hw {
    use embassy_stm32::{
        self as hal,
        adc::{resolution_to_max_count, Adc, AdcConfig, Averaging, Resolution, SampleTime},
    };

    /// Default sample time for potentiometer inputs.
    ///
    /// CYCLES387_5 (387.5 ADC clock cycles) provides adequate settling time for
    /// high-impedance pot sources on STM32H7. Per RM0468, this is sufficient for
    /// external analog signals.
    const POT_SAMPLE_TIME: SampleTime = SampleTime::CYCLES387_5;

    /// ADC resolution programmed for the pot inputs.
    ///
    /// Set **explicitly**. `AdcConfig::resolution: None` makes embassy skip the
    /// `ADC_CFGR.RES` write entirely, leaving the register at its reset value —
    /// which on STM32H7 is `Res::BITS16` (RES == 0b000), not the 12 bits that
    /// "12-bit ADC" habit suggests. See the note on [`super::normalize`].
    const POT_RESOLUTION: Resolution = Resolution::BITS16;

    /// Tie the normalisation divisor to the resolution actually programmed.
    ///
    /// This is the invariant whose breakage produced the TASK-018.04 knob bug:
    /// 16-bit conversions were divided by the 12-bit full scale (4095), so any
    /// reading past 6.25% of pot travel clamped to 1.0 and both knobs behaved
    /// as near-binary switches. A mismatch is now a build failure.
    const _: () = assert!(
        super::POT_FULL_SCALE_COUNTS == resolution_to_max_count(POT_RESOLUTION),
        "knob normalisation divisor must match the programmed ADC resolution"
    );

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
        /// Programs the ADC for 16-bit resolution ([`POT_RESOLUTION`]) and
        /// hardware averaging (`Samples16`) for jitter suppression.
        pub fn new(
            adc: impl Into<hal::Peri<'static, hal::peripherals::ADC1>>,
            knob1_pin: impl Into<hal::Peri<'static, hal::peripherals::PC4>>,
            knob2_pin: impl Into<hal::Peri<'static, hal::peripherals::PC0>>,
        ) -> Self {
            let adc = Adc::new_with_config(
                adc.into(),
                AdcConfig {
                    resolution: Some(POT_RESOLUTION),
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
        /// **increasing clockwise** — verified on hardware 2026-08-08, both
        /// knobs reaching exactly 0.0 and 1.0 at their physical stops. This is
        /// the conventional direction, so nothing is inverted here.
        ///
        /// Each read blocks for the duration of the ADC conversion (~105 µs with
        /// Samples16). Sequential reads of two channels take ~210 µs total, well
        /// within a 1 kHz polling budget.
        ///
        /// Safe to call from any non-audio-callback context.
        pub fn read(&mut self) -> (f32, f32) {
            let (raw1, raw2) = self.read_raw();
            (super::normalize(raw1), super::normalize(raw2))
        }

        /// Read both knobs as raw ADC counts, unnormalised.
        ///
        /// Full scale is [`super::POT_FULL_SCALE_COUNTS`], not 4095 — see the
        /// resolution note above before interpreting these.
        ///
        /// Exists for hardware bring-up and jitter measurement, where the
        /// normalised f32 loses the resolution that the measurement is about.
        /// Normal callers want [`Self::read`].
        pub fn read_raw(&mut self) -> (u16, u16) {
            let raw1 = self.adc.blocking_read(&mut self.knob1_pin, POT_SAMPLE_TIME);
            let raw2 = self.adc.blocking_read(&mut self.knob2_pin, POT_SAMPLE_TIME);
            (raw1, raw2)
        }
    }
}

#[cfg(feature = "pod-hw")]
pub use hw::Knobs;

// ---------------------------------------------------------------------------
// Tests — host-testable normalisation (no hardware feature needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_full_range() {
        // Full-scale reading maps to 1.0
        assert!((normalize(65535) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_zero() {
        // Zero reading maps to 0.0
        assert!((normalize(0) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_midpoint() {
        // Midpoint maps to ~0.5
        assert!((normalize(32768) - 0.5).abs() < 0.01);
    }

    #[test]
    fn twelve_bit_full_scale_is_not_full_scale() {
        // Regression test for the TASK-018.04 knob bug. Dividing a 16-bit
        // conversion by the 12-bit full scale clamped everything past 4095 —
        // 6.25% of pot travel — to 1.0. A pot 6% off its stop must read ~0.06,
        // not 1.0.
        let value = normalize(4095);
        assert!(
            (value - 0.0625).abs() < 0.001,
            "raw 4095 normalised to {value}, expected ~0.0625"
        );
    }

    #[test]
    fn monotonic_increase() {
        // Values increase monotonically with raw ADC reading
        let mut prev = 0.0_f32;
        for raw in 0..=u16::MAX {
            let value = normalize(raw);
            assert!(value >= prev, "not monotonic at raw={raw}");
            prev = value;
        }
    }

    #[test]
    fn spans_the_full_unit_interval() {
        // The endpoints are actually reachable — the property AC #1 of
        // TASK-018.04 checks by hand at the physical stops.
        assert_eq!(normalize(0), 0.0);
        assert_eq!(normalize(u16::MAX), 1.0);
    }
}
