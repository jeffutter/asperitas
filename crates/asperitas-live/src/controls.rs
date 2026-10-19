//! Lock-free hand-off of [`Controls`] from a control thread to the audio callback.

use std::sync::atomic::{AtomicU32, Ordering};

use asperitas_dsp::asperitas::{Controls, KNOB_COUNT};

/// One shared set of knob positions. The writer stores each knob as `f32` bits; the audio
/// callback reads them without locking or allocating. Knobs are independent atomics, so a read
/// concurrent with a write may mix old and new knobs for one block, which the effect's
/// smoothing absorbs (the firmware's knob state has the same shape).
pub struct ControlSlot {
    knobs: [AtomicU32; KNOB_COUNT],
}

impl ControlSlot {
    /// A slot holding `controls`.
    pub fn new(controls: &Controls) -> Self {
        Self {
            knobs: controls.knobs.map(|k| AtomicU32::new(k.to_bits())),
        }
    }

    /// Publish new knob positions from a control thread.
    pub fn write(&self, controls: &Controls) {
        for (slot, k) in self.knobs.iter().zip(controls.knobs) {
            slot.store(k.to_bits(), Ordering::Relaxed);
        }
    }

    /// Current knob positions, for the audio callback.
    pub fn read(&self) -> Controls {
        Controls {
            knobs: std::array::from_fn(|i| f32::from_bits(self.knobs[i].load(Ordering::Relaxed))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips() {
        let slot = ControlSlot::new(&Controls::default());
        assert_eq!(slot.read(), Controls::default());
        let c = Controls {
            knobs: [0.25, 0.75],
        };
        slot.write(&c);
        assert_eq!(slot.read(), c);
    }
}
