//! Daisy Pod control surface drivers for Asperitas firmware.
//!
//! Provides pin map type aliases and structs for the Pod's controls (buttons,
//! encoder, knobs, LEDs), following daisy-embassy's `pins_seed.rs` conventions
//! so the pin map can be contributed upstream unchanged.
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
pub mod pins;

#[cfg(feature = "pod-hw")]
pub mod knob;

#[cfg(feature = "pod-hw")]
pub mod led;

// ---------------------------------------------------------------------------
// Default init (no features)
// ---------------------------------------------------------------------------

/// Initialize the Pod subsystem with no-op defaults.
///
/// When no hardware feature is enabled, this does nothing. Call during boot
/// to keep the call-site uniform across feature configurations.
#[cfg(not(feature = "pod-hw"))]
pub fn init() {}
