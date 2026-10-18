//! The one saturating counter every wire-side statistic uses.

use core::sync::atomic::{AtomicU32, Ordering};

/// A `u32` event counter that stops at `u32::MAX` instead of rolling over.
///
/// A wrapped counter reads on the host as a huge negative rate and looks like a decoder bug, so
/// every statistic the device reports sits at the ceiling instead. `Relaxed` ordering is enough:
/// each counter is read as a snapshot and never used to synchronise other memory.
#[derive(Debug, Default)]
pub struct SaturatingCounter(AtomicU32);

impl SaturatingCounter {
    /// Zero, as at boot.
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// Add one.
    pub fn bump(&self) {
        self.add(1);
    }

    /// Add `n`, landing exactly on `u32::MAX` if the sum would pass it.
    pub fn add(&self, n: u32) {
        if n == 0 {
            return;
        }
        // The closure always returns `Some`, so the update cannot fail.
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(n))
            });
    }

    /// The current value.
    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_saturates_on_the_ceiling_instead_of_stalling_below_it() {
        let c = SaturatingCounter::new();
        c.add(u32::MAX - 1);
        c.add(5);
        assert_eq!(c.load(), u32::MAX);
        c.bump();
        assert_eq!(c.load(), u32::MAX);
    }

    #[test]
    fn add_zero_changes_nothing() {
        let c = SaturatingCounter::new();
        c.add(0);
        assert_eq!(c.load(), 0);
        c.bump();
        c.add(0);
        assert_eq!(c.load(), 1);
    }
}
