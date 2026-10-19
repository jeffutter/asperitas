//! Daisy Pod control surface drivers for Asperitas firmware.
//!
//! Provides control drivers for the Pod's encoder, knobs and LEDs. Firmware
//! passes board pins to them directly; there is no Pod pin map module.
//!
//! # Features
//!
//! - `pod-hw` — enables embassy-stm32 types (`Peri`, GPIO) for hardware builds.
//!   Without this feature the crate compiles to almost nothing on the host,
//!   matching the `asperitas-logging` pattern.

#![no_std]

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "pod-hw")]
pub mod led;

// Knob module: the ADC normalisation is always available for host testing;
// the `Knobs` driver itself requires pod-hw.
pub mod knob;

// Encoder module: algorithmic logic (LUT decoder, debouncer) is always
// available for host testing; ControlSurface requires pod-hw.
pub mod encoder;

// Ticker guard: pure arithmetic on Copy types, always available for host testing.
pub mod ticker_guard;

// Host override of knob values, upstream of DSP smoothing. Pure logic, host-testable.
pub mod knob_override;
