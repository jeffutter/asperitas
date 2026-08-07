//! Pod pin map — type aliases and struct for Daisy Pod controls.
//!
//! Mirrors daisy-embassy's `src/pins/pins_seed.rs` in shape and naming so this
//! can be contributed upstream as-is once proven on hardware.
//!
//! Pin assignments are transcribed from libDaisy's `daisy_pod.cpp` (Rev3/Rev4
//! pinout) and verified against the Seed3 D-pin designators. See
//! `docs/reference/daisy-pod.md` for the authoritative table.

use embassy_stm32::{self as hal, Peri};
use hal::peripherals::*;

// - type aliases -------------------------------------------------------------

/// Button 1 (`SW_1`) — Pod D27 = PG9
pub type PodSw1<'a> = Peri<'a, PG9>;

/// Button 2 (`SW_2`) — Pod D28 = PA2
pub type PodSw2<'a> = Peri<'a, PA2>;

/// Encoder channel A — Pod D26 = PD11
pub type PodEncA<'a> = Peri<'a, PD11>;

/// Encoder channel B — Pod D25 = PA0
pub type PodEncB<'a> = Peri<'a, PA0>;

/// Encoder push switch — Pod D13 = PB6
pub type PodEncSw<'a> = Peri<'a, PB6>;

/// LED 2 red channel — Pod D17 = PB1
pub type PodLed2Red<'a> = Peri<'a, PB1>;

/// LED 2 green channel — Pod D24 = PA1
pub type PodLed2Green<'a> = Peri<'a, PA1>;

/// LED 2 blue channel — Pod D23 = PA4
pub type PodLed2Blue<'a> = Peri<'a, PA4>;

/// Knob 1 (potentiometer) — Pod D21 = PC4
pub type PodKnob1<'a> = Peri<'a, PC4>;

/// Knob 2 (potentiometer) — Pod D15 = PC0
pub type PodKnob2<'a> = Peri<'a, PC0>;

// - struct -------------------------------------------------------------------

/// All Pod control pins collected into one struct.
///
/// Like daisy-embassy's [`DaisyPins`](hal::pins::seed::DaisyPins), each field
/// owns a `Peri` handle that must be consumed by exactly one driver.
#[allow(non_snake_case)]
pub struct PodPins<'a> {
    /// Button 1 (`SW_1`) — D27 = PG9
    pub sw_1: PodSw1<'a>,
    /// Button 2 (`SW_2`) — D28 = PA2
    pub sw_2: PodSw2<'a>,
    /// Rotary encoder channel A — D26 = PD11
    pub enc_a: PodEncA<'a>,
    /// Rotary encoder channel B — D25 = PA0
    pub enc_b: PodEncB<'a>,
    /// Rotary encoder push switch — D13 = PB6
    pub enc_sw: PodEncSw<'a>,
    /// LED 2 red — D17 = PB1
    pub led2_r: PodLed2Red<'a>,
    /// LED 2 green — D24 = PA1
    pub led2_g: PodLed2Green<'a>,
    /// LED 2 blue — D23 = PA4
    pub led2_b: PodLed2Blue<'a>,
    /// Knob 1 (ADC input) — D21 = PC4
    pub knob1: PodKnob1<'a>,
    /// Knob 2 (ADC input) — D15 = PC0
    pub knob2: PodKnob2<'a>,
}
