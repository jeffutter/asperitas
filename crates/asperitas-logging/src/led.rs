//! LED boot-stage indicator for the Daisy Pod.
//!
//! Controls one of the Pod's RGB LEDs as a status indicator, with states for
//! pre-init, running, and panicked conditions. Polarity is controlled by a
//! single constant so TASK-006.02 can flip it without code changes.

use embassy_stm32::{
    self as hal,
    gpio::{Level, Output, Speed},
    Peri,
};
use embassy_time::Timer;

/// LED polarity — set to `true` if the Pod's LEDs are active-low.
///
/// libDaisy's LED driver inverts polarity, so a naive port can produce
/// LEDs that are on when you expect off. TASK-006.02 will determine the
/// correct value. Defaulting to `true` (active-low) to match libDaisy.
pub(crate) const LED_ACTIVE_LOW: bool = true;

/// Boot-stage states for the LED indicator.
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum LedState {
    /// Slow blink red (~1 Hz) — firmware is starting up.
    PreInit,
    /// Steady green — audio pipeline active, system running normally.
    Running,
    /// Fast strobe red (~5 Hz) — unrecoverable error / panic.
    Panicked,
}

/// RGB LED driver for boot-stage indication.
///
/// Uses LED 1 on the Pod (D20/D19/D18 = PC1/PA6/PA7).
/// Simple GPIO output (not PWM) — on/off per color channel is sufficient
/// for visually distinct states.
pub struct BootLed {
    red: Output<'static>,
    green: Output<'static>,
    blue: Output<'static>,
}

impl BootLed {
    /// Create a new `BootLed` from raw GPIO pins.
    ///
    /// # Arguments
    ///
    /// * `red_pin` — Red channel pin (Pod D20 = PC1)
    /// * `green_pin` — Green channel pin (Pod D19 = PA6)
    /// * `blue_pin` — Blue channel pin (Pod D18 = PA7)
    pub fn new(
        red_pin: Peri<'static, hal::peripherals::PC1>,
        green_pin: Peri<'static, hal::peripherals::PA6>,
        blue_pin: Peri<'static, hal::peripherals::PA7>,
    ) -> Self {
        let _active = if LED_ACTIVE_LOW {
            Level::Low
        } else {
            Level::High
        };
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

    /// Set the LED to a specific state synchronously.
    ///
    /// For blinking states (PreInit, Panicked), this sets the LED to its
    /// "on" phase. Use [`blink_task`] for animated blinking.
    pub fn set_state(&mut self, state: LedState) {
        match state {
            LedState::PreInit => {
                // Red on, others off
                self.set_color(true, false, false);
            }
            LedState::Running => {
                // Green on, others off
                self.set_color(false, true, false);
            }
            LedState::Panicked => {
                // Red on, others off (same as PreInit but different blink rate)
                self.set_color(true, false, false);
            }
        }
    }

    /// Set individual color channels.
    fn set_color(&mut self, red: bool, green: bool, blue: bool) {
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

        if red {
            self.red.set_level(on);
        } else {
            self.red.set_level(off);
        }
        if green {
            self.green.set_level(on);
        } else {
            self.green.set_level(off);
        }
        if blue {
            self.blue.set_level(on);
        } else {
            self.blue.set_level(off);
        }
    }

    /// Turn all LEDs off.
    pub fn off(&mut self) {
        let off = if LED_ACTIVE_LOW {
            Level::High
        } else {
            Level::Low
        };
        self.red.set_level(off);
        self.green.set_level(off);
        self.blue.set_level(off);
    }

    /// Async blink task for a given state.
    ///
    /// Consumes `self` and loops forever, toggling the LED at the appropriate
    /// rate for the given state:
    /// - `PreInit`: ~1 Hz (500 ms on, 500 ms off)
    /// - `Panicked`: ~5 Hz (100 ms on, 100 ms off)
    /// - `Running`: sets solid green and returns immediately
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In main():
    /// let led = BootLed::new(board.pins.d20, board.pins.d19, board.pins.d18);
    /// spawner.spawn(async {
    ///     led.blink_task(LedState::PreInit).await;
    /// }).ok();
    ///
    /// // Later, when audio starts:
    /// led.set_state(LedState::Running);
    /// ```
    pub async fn blink_task(mut self, state: LedState) {
        match state {
            LedState::Running => {
                // Solid green, no blink loop needed
                self.set_state(LedState::Running);
                return;
            }
            LedState::PreInit | LedState::Panicked => {}
        }

        let (on_ms, off_ms) = match state {
            LedState::PreInit => (500, 500),  // ~1 Hz
            LedState::Panicked => (100, 100), // ~5 Hz
            LedState::Running => unreachable!(),
        };

        loop {
            // Turn red on
            self.set_color(true, false, false);
            Timer::after_millis(on_ms).await;

            // Turn all off
            self.off();
            Timer::after_millis(off_ms).await;
        }
    }

    /// Set the panicked state and enter an infinite blink loop.
    ///
    /// This is called from the panic handler path (via critical section)
    /// or directly when a fatal error occurs. It does NOT return.
    #[allow(dead_code)]
    pub fn panic_loop(mut self) -> ! {
        // During panic, we can't use async. Just set red on and loop.
        // The blink won't be animated, but the LED will be visible.
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

        self.red.set_level(on);
        self.green.set_level(off);
        self.blue.set_level(off);

        cortex_m::asm::bkpt();
        loop {
            cortex_m::asm::nop();
        }
    }
}
