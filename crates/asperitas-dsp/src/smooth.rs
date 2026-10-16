//! Parameter smoothing utilities.
//!
//! One-pole exponential smoothing with a time-constant-based alpha. Keeps parameter
//! transitions free of clicks and pops.

use libm::expf as exp;

/// Magnitudes below this are flushed to zero. It is far below any audible level (-600 dB
/// relative to full scale) and far above the subnormal range, so exponential tails reach exactly
/// zero without depending on the host's flush-to-zero flags.
const FLUSH_THRESHOLD: f32 = 1e-30;

/// Replace a non-finite or near-zero value with `0.0`.
///
/// Applied to input samples and to recursive state so a NaN or infinity cannot persist in a
/// feedback path and decaying tails never linger in the subnormal range.
#[inline]
pub(crate) fn sanitize(x: f32) -> f32 {
    if x.is_finite() && x.abs() >= FLUSH_THRESHOLD {
        x
    } else {
        0.0
    }
}

/// Exponential smoother for a single floating-point value.
///
/// Derived from a time constant (milliseconds): the value reaches ~63 % of a step change
/// after `time_ms` milliseconds at the current sample rate.
pub(crate) struct Smoother {
    current: f32,
    target: f32,
    alpha: f32,
}

impl Smoother {
    /// Create a new smoother starting at `initial` with the given time constant and sample rate.
    ///
    /// `time_ms` — smoothing time constant in milliseconds (> 0).
    /// `sample_rate_hz` — audio sample rate (> 0).
    pub fn new(initial: f32, time_ms: f32, sample_rate_hz: f32) -> Self {
        // alpha = 1 - exp(-1 / (time_constant_samples))
        let time_constant_samples = time_ms * sample_rate_hz / 1000.0;
        let alpha = 1.0 - exp(-1.0 / time_constant_samples);
        Self {
            current: initial,
            target: initial,
            alpha,
        }
    }

    /// Update the target value. Call this whenever the parameter changes.
    ///
    /// A non-finite value is ignored, so the previous target stays in force rather than
    /// poisoning the smoothed value.
    pub fn set_target(&mut self, val: f32) {
        if val.is_finite() {
            self.target = val;
        }
    }

    /// Advance the smoother by one sample and return the smoothed value.
    pub fn advance(&mut self) -> f32 {
        self.current = sanitize(self.current + self.alpha * (self.target - self.current));
        self.current
    }

    /// Snap current to target instantly. Used during reset or when disabling smoothing.
    pub fn snap(&mut self) {
        self.current = self.target;
    }

    /// Update the alpha coefficient when sample rate changes.
    pub fn update_sample_rate(&mut self, time_ms: f32, sample_rate_hz: f32) {
        let time_constant_samples = time_ms * sample_rate_hz / 1000.0;
        self.alpha = 1.0 - exp(-1.0 / time_constant_samples);
    }
}
