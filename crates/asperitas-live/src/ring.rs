//! The line-in ring's fill policy, free of audio-device types so a test can drive it.
//!
//! The input and output streams run on independent clocks, so the ring drifts: toward empty it
//! starves the output, toward full it holds the signal back as latency. [`RingSync`] decides, once
//! per output callback, whether to wait for the ring to refill, play, or drop surplus.

/// What the output callback does with the ring this callback.
#[derive(Debug, PartialEq, Eq)]
pub enum RingAction {
    /// Too little queued: emit silence and leave the ring to fill.
    Wait,
    /// Play from the ring.
    Run,
    /// Drift: drop this many of the oldest queued frames, then play.
    Trim(usize),
    /// Priming surplus: drop this many of the oldest queued frames, then play. Not drift.
    Prime(usize),
}

/// Fill policy for a ring of fixed capacity. Primed means one host buffer has been queued since
/// the start or the last underrun.
pub struct RingSync {
    capacity: usize,
    primed: bool,
}

impl RingSync {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            primed: false,
        }
    }

    /// Fill level above which the surplus is dropped: half the capacity, but never less than two
    /// host buffers, so a steady stream is not trimmed on every callback.
    fn high_water(&self, wanted: usize) -> usize {
        (self.capacity / 2).max(2 * wanted).min(self.capacity)
    }

    /// Fill level (before the pop) a trim leaves behind: `wanted` plus half the headroom to the
    /// high-water mark, so the ring is not empty after the callback and the next callback does
    /// not trim again.
    fn trim_target(&self, wanted: usize) -> usize {
        wanted + (self.high_water(wanted) - wanted) / 2
    }

    /// Decide for a callback that needs `wanted` frames while `occupied` are queued.
    pub fn decide(&mut self, occupied: usize, wanted: usize) -> RingAction {
        if !self.primed {
            if occupied < wanted {
                return RingAction::Wait;
            }
            self.primed = true;
            return match occupied.saturating_sub(self.trim_target(wanted)) {
                0 => RingAction::Run,
                surplus => RingAction::Prime(surplus),
            };
        }
        if occupied > self.high_water(wanted) {
            RingAction::Trim(occupied - self.trim_target(wanted))
        } else {
            RingAction::Run
        }
    }

    /// True when a callback of `wanted` frames can never prime: the ring holds fewer than two
    /// such buffers, so waiting for it to fill would last forever.
    pub fn cannot_prime(&self, wanted: usize) -> bool {
        wanted * 2 > self.capacity
    }

    /// The callback found the ring short: hold back until a host buffer is queued again.
    pub fn underrun(&mut self) {
        self.primed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_until_one_buffer_is_queued_then_primes_to_a_margin() {
        let mut sync = RingSync::new(4096);
        assert_eq!(sync.decide(0, 128), RingAction::Wait);
        assert_eq!(sync.decide(127, 128), RingAction::Wait);
        assert_eq!(sync.decide(128, 128), RingAction::Run);
        let mut sync = RingSync::new(4096);
        assert_eq!(sync.decide(1000, 128), RingAction::Run);
        let mut sync = RingSync::new(4096);
        assert_eq!(sync.decide(1100, 128), RingAction::Prime(12));
    }

    #[test]
    fn cannot_prime_when_the_ring_holds_under_two_buffers() {
        let sync = RingSync::new(4096);
        assert!(!sync.cannot_prime(2048));
        assert!(sync.cannot_prime(2049));
    }

    #[test]
    fn re_primes_after_an_underrun() {
        let mut sync = RingSync::new(4096);
        assert_eq!(sync.decide(128, 128), RingAction::Run);
        sync.underrun();
        assert_eq!(sync.decide(10, 128), RingAction::Wait);
        assert_eq!(sync.decide(300, 128), RingAction::Run);
    }

    #[test]
    fn trims_to_a_margin_above_wanted_past_the_high_water_mark() {
        let mut sync = RingSync::new(4096);
        sync.decide(128, 128);
        assert_eq!(sync.decide(2048, 128), RingAction::Run);
        assert_eq!(sync.decide(2049, 128), RingAction::Trim(961));
        // The ring still holds a buffer after the pop, and the next callback does not trim.
        assert_eq!(sync.decide(1088 - 128 + 64, 128), RingAction::Run);
    }

    #[test]
    fn high_water_keeps_two_buffers_when_the_host_buffer_is_large() {
        let mut sync = RingSync::new(4096);
        sync.decide(1500, 1500);
        assert_eq!(sync.decide(3000, 1500), RingAction::Run);
        assert_eq!(sync.decide(3001, 1500), RingAction::Trim(751));
    }
}
