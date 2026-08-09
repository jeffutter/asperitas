//! Ticker overrun guard — shared policy for bounding catch-up bursts.
//!
//! `embassy_time::Ticker::next()` replays all missed ticks back-to-back when the
//! executor stalls (e.g., under a debugger). This module provides a predicate that
//! callers use to detect overruns and reset the ticker, discarding the backlog.
//!
//! # Policy
//!
//! If the time since the last poll exceeds **twice** the poll period, the gap is
//! treated as an overrun and the caller should call `Ticker::reset()` before
//! awaiting `Ticker::next()`. The threshold is strictly greater-than: a gap of
//! exactly `2 * period` does NOT trigger a reset.
//!
//! # Call-site sequencing
//!
//! After detecting an overrun and calling `Ticker::reset()`, the caller MUST
//! still await `Ticker::next()`. Without this, the loop could spin through an
//! extra zero-wait iteration. The correct sequence is:
//!
//! ```text
//! let now = Instant::now();
//! if should_reset(now, last_tick, period) {
//!     ticker.reset();
//! }
//! ticker.next().await;
//! last_tick = now;
//! ```
//!
//! This guarantees at most one immediate (near-zero-wait) poll iteration after
//! a stall, never zero and never two or more.

use embassy_time::{Duration, Instant};

/// Return true if the ticker should be reset due to an overrun.
///
/// An overrun is detected when the elapsed time since `last_tick` exceeds
/// `2 * period`. The comparison is strictly greater-than: a gap of exactly
/// `2 * period` does not trigger a reset.
///
/// # Arguments
///
/// * `now` — current instant (typically `embassy_time::Instant::now()`)
/// * `last_tick` — instant recorded at the end of the previous poll iteration
/// * `period` — the nominal poll period (e.g., 1 ms for a 1 kHz poll loop)
pub fn should_reset(now: Instant, last_tick: Instant, period: Duration) -> bool {
    let elapsed = now - last_tick;
    elapsed > period * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct an Instant from milliseconds using consistent tick scaling.
    fn instant_from_millis(ms: u64) -> Instant {
        Instant::from_ticks(ms * 1_000)
    }

    fn duration_from_millis(ms: u64) -> Duration {
        Duration::from_ticks(ms * 1_000)
    }

    #[test]
    fn boundary_condition_tests() {
        // period = 1 ms → threshold is 2 ms (strictly greater-than)
        let period = duration_from_millis(1);

        // gap == period (normal operation) → no reset
        assert!(!should_reset(
            instant_from_millis(1),
            instant_from_millis(0),
            period
        ));

        // gap == 2 * period (exact boundary) → no reset (strictly greater-than)
        assert!(!should_reset(
            instant_from_millis(2),
            instant_from_millis(0),
            period
        ));

        // gap > 2 * period (overrun detected) → reset
        assert!(should_reset(
            instant_from_millis(3),
            instant_from_millis(0),
            period
        ));

        // large stall → reset
        assert!(should_reset(
            instant_from_millis(1000),
            instant_from_millis(0),
            period
        ));

        // sub-threshold relative to long period → no reset
        let long_period = duration_from_millis(500);
        assert!(!should_reset(
            instant_from_millis(1),
            instant_from_millis(0),
            long_period
        ));

        // exactly at boundary with long period → no reset
        assert!(!should_reset(
            instant_from_millis(1000),
            instant_from_millis(0),
            long_period
        ));

        // just over boundary with long period → reset
        assert!(should_reset(
            instant_from_millis(1001),
            instant_from_millis(0),
            long_period
        ));
    }

    #[test]
    fn guard_prevents_cascade_after_stall() {
        // Simulate the poll loop structure to prove the guard correctly
        // identifies the stall point and does not re-trigger on normal gaps.
        //
        // Before guard (TASK-026 bug): Ticker::next() fires ~97 times back-to-back
        // after a 97ms stall, each with near-zero elapsed time.
        //
        // With guard: should_reset() returns true once at the stall boundary,
        // Ticker::reset() discards backlog, Ticker::next() fires once, then
        // normal spacing resumes.
        let period = duration_from_millis(1);

        // Just before stall: tick at t=3
        let before_stall = instant_from_millis(3);
        // Right after stall: t=100 (97ms gap)
        let after_stall = instant_from_millis(100);

        // The guard detects the overrun
        assert!(should_reset(after_stall, before_stall, period));

        // After reset+tick at t=100, the next poll at t=101 has a normal
        // gap (1ms), so no further reset is triggered
        let next_normal = instant_from_millis(101);
        assert!(!should_reset(next_normal, after_stall, period));

        // And subsequent iterations also stay normal
        assert!(!should_reset(instant_from_millis(102), next_normal, period));
    }
}
