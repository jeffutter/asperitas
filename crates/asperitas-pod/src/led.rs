//! LED 2 driver for the Daisy Pod.
//!
//! Controls the second RGB LED on the Pod (D17/D24/D23 = PB1/PA1/PA4) using
//! simple GPIO on/off per channel. The Pod's LEDs are active-low: driving a
//! pin `Low` lights its channel, `High` turns it off.
//!
//! Unlike [`asperitas-logging::led::BootLed`] which owns LED 1 as a singleton
//! for boot-stage indication, this is a regular component — the consumer decides
//! storage strategy and lifecycle. No static cell, no global state.
//!
//! # Drive method: on/off only (no PWM)
//!
//! Hardware PWM requires one timer channel per colour pin. Per STM32H750 reference
//! manual (RM0468) and ST CubeMX PeripheralPins.c:
//!
//! - **PB1** (LED 2 red): TIM1_CH3N, TIM3_CH4, TIM8_CH3N — has timer channels
//! - **PA1** (LED 2 green): TIM2_CH2, TIM5_CH2, TIM15_CH1N — has timer channels
//! - **PA4** (LED 2 blue): DAC_OUT1 only — **no TIM alternate function**
//!
//! Since PA4 has no timer channel, uniform hardware PWM across all three colours
//! is not possible. Software PWM would require a dedicated embassy task with no
//! clear owner yet. Per doc-001 §3 ("abstractions must justify their cost"),
//! on/off gives seven distinct colours (Off + R/G/B/Yellow/Cyan/Magenta/White),
//! which is sufficient for a status indicator without adding PWM complexity.

use embassy_stm32::{
    self as hal,
    gpio::{Level, Output, Speed},
    Peri,
};

/// LED polarity — the Pod's RGB LEDs are active-low: driving a channel pin
/// `Low` lights it, `High` turns it off. Verified on hardware via
/// `firmware/src/bin/ledtest.rs` (see docs/reference/daisy-pod.md).
pub const LED_ACTIVE_LOW: bool = true;

/// Colours available with on/off drive (no PWM).
///
/// Seven combinations from three binary channels, plus Off.
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Led2Color {
    /// All channels off.
    Off,
    /// Red only.
    Red,
    /// Green only.
    Green,
    /// Blue only.
    Blue,
    /// Red + Green.
    Yellow,
    /// Green + Blue.
    Cyan,
    /// Red + Blue.
    Magenta,
    /// Red + Green + Blue.
    White,
}

/// RGB LED 2 driver — simple GPIO on/off per channel.
///
/// Consumes ownership of the three LED pins at construction time.
pub struct Led2 {
    red: Output<'static>,
    green: Output<'static>,
    blue: Output<'static>,
}

impl Led2 {
    /// Create a new LED 2 driver from the three RGB pins.
    ///
    /// Pins are initialized to the off level (active-high = `High` for active-low LEDs).
    pub fn new(
        red_pin: Peri<'static, hal::peripherals::PB1>,
        green_pin: Peri<'static, hal::peripherals::PA1>,
        blue_pin: Peri<'static, hal::peripherals::PA4>,
    ) -> Self {
        let inactive = if LED_ACTIVE_LOW {
            Level::High
        } else {
            Level::Low
        };

        Self {
            red: Output::new(red_pin, inactive, Speed::Low),
            green: Output::new(green_pin, inactive, Speed::Low),
            blue: Output::new(blue_pin, inactive, Speed::Low),
        }
    }

    /// Set the LED colour.
    ///
    /// Immediately drives the pins — does not require any background task.
    pub fn set_color(&mut self, color: Led2Color) {
        let (r, g, b) = match color {
            Led2Color::Off => (false, false, false),
            Led2Color::Red => (true, false, false),
            Led2Color::Green => (false, true, false),
            Led2Color::Blue => (false, false, true),
            Led2Color::Yellow => (true, true, false),
            Led2Color::Cyan => (false, true, true),
            Led2Color::Magenta => (true, false, true),
            Led2Color::White => (true, true, true),
        };
        self.set_channels(r, g, b);
    }

    /// Turn all channels off.
    pub fn off(&mut self) {
        self.set_color(Led2Color::Off);
    }

    /// Set individual colour channels.
    ///
    /// `true` = light the channel (drives pin Low for active-low LEDs).
    pub fn set_channels(&mut self, red: bool, green: bool, blue: bool) {
        let on = if LED_ACTIVE_LOW {
            Level::Low
        } else {
            Level::High
        };
        let off = if LED_ACTIVE_LOW {
            Level::High
        } else {
            Level::Low
        };

        self.red.set_level(if red { on } else { off });
        self.green.set_level(if green { on } else { off });
        self.blue.set_level(if blue { on } else { off });
    }
}
