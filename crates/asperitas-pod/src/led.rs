//! RGB LED drivers for the Daisy Pod.
//!
//! [`RgbLed`] is the one channel driver for both Pod LEDs: simple GPIO on/off per
//! channel, with the Pod's active-low polarity defined in one place. [`Led2`] wraps
//! it for LED 2 (D17/D24/D23 = PB1/PA1/PA4) with named colours; `asperitas-logging`'s
//! boot LED wraps it for LED 1 (PC1/PA6/PA7).
//!
//! The drivers are regular components - the consumer decides storage strategy and
//! lifecycle. No static cell, no global state.
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
//! clear owner yet, so
//! on/off gives seven distinct colours (Off + R/G/B/Yellow/Cyan/Magenta/White),
//! which is sufficient for a status indicator without adding PWM complexity.

use embassy_stm32::{
    self as hal,
    gpio::{Level, Output, Pin, Speed},
    Peri,
};

/// LED polarity — the Pod's RGB LEDs are active-low: driving a channel pin
/// `Low` lights it, `High` turns it off. This matches libDaisy. Verified on
/// hardware via `firmware/src/bin/ledtest.rs`, which drives the pins directly
/// (see docs/reference/daisy-pod.md).
const LED_ACTIVE_LOW: bool = true;

/// One Pod RGB LED: three GPIO channels, on/off each, polarity handled here.
pub struct RgbLed {
    red: Output<'static>,
    green: Output<'static>,
    blue: Output<'static>,
}

impl RgbLed {
    /// Take the three channel pins, all initially off.
    pub fn new(
        red_pin: Peri<'static, impl Pin>,
        green_pin: Peri<'static, impl Pin>,
        blue_pin: Peri<'static, impl Pin>,
    ) -> Self {
        let off = Self::level(false);
        Self {
            red: Output::new(red_pin, off, Speed::Low),
            green: Output::new(green_pin, off, Speed::Low),
            blue: Output::new(blue_pin, off, Speed::Low),
        }
    }

    /// Pin level that lights (`true`) or darkens (`false`) a channel.
    fn level(lit: bool) -> Level {
        if lit == LED_ACTIVE_LOW {
            Level::Low
        } else {
            Level::High
        }
    }

    /// Set the channels; `true` lights the channel. Drives the pins immediately.
    pub fn set_channels(&mut self, red: bool, green: bool, blue: bool) {
        self.red.set_level(Self::level(red));
        self.green.set_level(Self::level(green));
        self.blue.set_level(Self::level(blue));
    }

    /// Turn all channels off.
    pub fn off(&mut self) {
        self.set_channels(false, false, false);
    }
}

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

/// RGB LED 2 driver - named colours over an [`RgbLed`].
///
/// Consumes ownership of the three LED pins at construction time.
pub struct Led2 {
    led: RgbLed,
}

impl Led2 {
    /// Create a new LED 2 driver from the three RGB pins, initially off.
    pub fn new(
        red_pin: Peri<'static, hal::peripherals::PB1>,
        green_pin: Peri<'static, hal::peripherals::PA1>,
        blue_pin: Peri<'static, hal::peripherals::PA4>,
    ) -> Self {
        Self {
            led: RgbLed::new(red_pin, green_pin, blue_pin),
        }
    }

    /// Set the LED colour.
    ///
    /// Immediately drives the pins - does not require any background task.
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
        self.led.set_channels(r, g, b);
    }
}
