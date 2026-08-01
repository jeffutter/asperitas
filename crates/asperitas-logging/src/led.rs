//! LED boot-stage indicator for the Daisy Pod.
//!
//! Controls one of the Pod's RGB LEDs as a status indicator, with states for
//! pre-init, running, and panicked conditions. Polarity is controlled by a
//! single constant so TASK-006.02 can flip it without code changes.
//!
//! # Architecture
//!
//! A singleton [`BootLed`] is created once by [`init`], stored in static
//! storage via [`StaticCell`]. Both the async execution path (main/blinky)
//! and the synchronous panic handler access the same instance through
//! [`get_mut`]. An [`AtomicU32`] coordinates state transitions between
//! the async [`blink_task`] and interrupt/panic context.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_stm32::{
    self as hal,
    gpio::{Level, Output, Speed},
    Peri,
};
use embassy_time::Timer;
use static_cell::StaticCell;

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
    /// Red on — unrecoverable error / panic.
    Panicked,
}

impl LedState {
    /// Convert from u32 atomic value. Defaults to `PreInit` on unknown values.
    fn from_u32(val: u32) -> Self {
        match val {
            0 => LedState::PreInit,
            1 => LedState::Running,
            _ => LedState::Panicked,
        }
    }

    /// Convert to u32 for atomic storage.
    fn to_u32(self) -> u32 {
        match self {
            LedState::PreInit => 0,
            LedState::Running => 1,
            LedState::Panicked => 2,
        }
    }
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

// ---------------------------------------------------------------------------
// Singleton storage — initialized once by init(), accessed via get_mut()
// ---------------------------------------------------------------------------

static BOOT_LED_STORAGE: StaticCell<BootLed> = StaticCell::new();

/// Cached mutable reference to the initialized BootLed.
/// Set once by init(), read by get_mut(). Safe on single-core Cortex-M.
static mut BOOT_LED_REF: *mut BootLed = core::ptr::null_mut();

/// Global atomic state — read by blink_task, written by main() and panic_handler.
static LED_STATE: AtomicU32 = AtomicU32::new(0); // 0 = PreInit (default)

/// Initialize the singleton BootLed.
///
/// Consumes the three RGB LED pins and stores the driver in static storage.
/// Sets the initial state to `PreInit` (red blink at ~1 Hz).
///
/// Call this once during boot, before spawning the blink_task or any panic
/// can occur. Both the async execution path and the panic handler share
/// this single instance.
///
/// # Arguments
///
/// * `red_pin` — Red channel pin (Pod D20 = PC1)
/// * `green_pin` — Green channel pin (Pod D19 = PA6)
/// * `blue_pin` — Blue channel pin (Pod D18 = PA7)
///
/// # Panics
///
/// Panics if called more than once.
pub fn init(
    red_pin: Peri<'static, hal::peripherals::PC1>,
    green_pin: Peri<'static, hal::peripherals::PA6>,
    blue_pin: Peri<'static, hal::peripherals::PA7>,
) {
    let inactive = if LED_ACTIVE_LOW {
        Level::High
    } else {
        Level::Low
    };

    let led = BootLed {
        red: Output::new(red_pin, inactive, Speed::Low),
        green: Output::new(green_pin, inactive, Speed::Low),
        blue: Output::new(blue_pin, inactive, Speed::Low),
    };

    let boot_led = BOOT_LED_STORAGE.init(led);
    unsafe {
        BOOT_LED_REF = boot_led;
    }

    // Start in PreInit state
    LED_STATE.store(LedState::PreInit.to_u32(), Ordering::Release);
}

/// Get a mutable reference to the singleton BootLed.
///
/// # Safety
///
/// Must only be called after [`init`] has been called. Safe on single-core
/// Cortex-M with controlled init-then-access lifecycle.
pub fn get_mut() -> &'static mut BootLed {
    unsafe { &mut *BOOT_LED_REF }
}

/// Set the global LED state atomically.
///
/// Updates both the atomic state (read by blink_task) and immediately calls
/// [`BootLed::set_state`] for synchronous effect. Used by main.rs/blinky.rs
/// for explicit state transitions.
pub fn set_global_state(state: LedState) {
    LED_STATE.store(state.to_u32(), Ordering::Release);
    get_mut().set_state(state);
}

impl BootLed {
    /// Set the LED to a specific state synchronously.
    pub fn set_state(&mut self, state: LedState) {
        match state {
            LedState::PreInit => {
                // Red on, others off
                self.set_color_on(true, false, false);
            }
            LedState::Running => {
                // Green on, others off
                self.set_color_on(false, true, false);
            }
            LedState::Panicked => {
                // Red on, others off
                self.set_color_on(true, false, false);
            }
        }
    }

    /// Set individual color channels.
    pub fn set_color_on(&mut self, red: bool, green: bool, blue: bool) {
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
}

/// Async blink task that reads the global atomic state each iteration.
///
/// Accesses the singleton [`BootLed`] via raw pointer internally, so it takes
/// no arguments and can be freely selected alongside other futures without
/// borrow-across-await issues.
///
/// Runs forever — does not return.
///
/// - `PreInit`: blink red at ~1 Hz
/// - `Running`: steady green (polls for state transitions)
///
/// Note: `Panicked` is not handled here. The panic handler sets the LED to
/// steady red synchronously via [`set_global_state`] and then halts the
/// async executor, so this task is never polled after a real panic.
///
/// # Example
///
/// ```ignore
/// // In main():
/// asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);
///
/// // Run alongside other futures via select:
/// let led_fut = asperitas_logging::led::blink_task();
/// embassy_futures::select!(led_fut, other_fut).await;
/// ```
pub async fn blink_task() {
    loop {
        let state = LedState::from_u32(LED_STATE.load(Ordering::Acquire));
        let led = unsafe { &mut *BOOT_LED_REF };

        match state {
            LedState::Running => {
                // Steady green — yield periodically to stay responsive
                led.set_state(LedState::Running);
                Timer::after_millis(500).await;
            }
            LedState::PreInit => {
                // Red blink ~1 Hz
                led.set_color_on(true, false, false);
                Timer::after_millis(500).await;
                led.off();
                Timer::after_millis(500).await;
            }
            LedState::Panicked => {
                // Unreachable in practice: the panic handler sets steady red
                // synchronously and then halts the async executor.
                // Kept as exhaustive match arm to avoid compiler warnings.
                led.set_state(LedState::Panicked);
                Timer::after_millis(1000).await;
            }
        }
    }
}
