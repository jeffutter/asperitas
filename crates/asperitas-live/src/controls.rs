//! Lock-free hand-off of a control surface's knob positions from a control thread to the audio
//! callback.

use std::sync::atomic::{AtomicU32, Ordering};

use asperitas_dsp::automation::ControlSurface;

/// Most knobs any control surface has (the resonator's thirteen).
pub const MAX_KNOBS: usize = asperitas_dsp::RESONATOR_KNOB_COUNT;

/// One shared set of knob positions for whichever effect runs. The writer stores each knob as
/// `f32` bits; the audio callback reads them without locking or allocating. Knobs are independent
/// atomics, so a read concurrent with a write may mix old and new knobs for one block, which the
/// effect's smoothing absorbs (the firmware's knob state has the same shape). The slot is
/// created for one surface and read and written as that surface.
pub struct ControlSlot {
    knobs: [AtomicU32; MAX_KNOBS],
    len: usize,
}

impl ControlSlot {
    /// A slot holding `controls`. Panics when the surface has more than [`MAX_KNOBS`] knobs.
    pub fn new<C: ControlSurface>(controls: &C) -> Self {
        let positions = controls.positions();
        assert!(positions.len() <= MAX_KNOBS, "surface has too many knobs");
        Self {
            knobs: std::array::from_fn(|i| {
                AtomicU32::new(positions.get(i).copied().unwrap_or(0.0).to_bits())
            }),
            len: positions.len(),
        }
    }

    /// Publish new knob positions from a control thread.
    pub fn write<C: ControlSurface>(&self, controls: &C) {
        debug_assert_eq!(controls.positions().len(), self.len);
        for (slot, k) in self.knobs.iter().zip(controls.positions()) {
            slot.store(k.to_bits(), Ordering::Relaxed);
        }
    }

    /// Current knob positions, for the audio callback.
    pub fn read<C: ControlSurface>(&self) -> C {
        debug_assert_eq!(C::PARAMS.len(), self.len);
        let mut controls = C::default();
        for (p, slot) in controls.positions_mut().iter_mut().zip(&self.knobs) {
            *p = f32::from_bits(slot.load(Ordering::Relaxed));
        }
        controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_dsp::asperitas::Controls;
    use asperitas_dsp::resonator_controls::ResonatorControls;

    #[test]
    fn write_then_read_round_trips() {
        let slot = ControlSlot::new(&Controls::default());
        assert_eq!(slot.read::<Controls>(), Controls::default());
        let c = Controls {
            knobs: [0.25, 0.75],
        };
        slot.write(&c);
        assert_eq!(slot.read::<Controls>(), c);
    }

    #[test]
    fn resonator_positions_round_trip() {
        let slot = ControlSlot::new(&ResonatorControls::default());
        assert_eq!(
            slot.read::<ResonatorControls>(),
            ResonatorControls::default()
        );
        let c = ResonatorControls {
            positions: std::array::from_fn(|i| 0.1 * (i + 1) as f32 / 2.0),
        };
        slot.write(&c);
        assert_eq!(slot.read::<ResonatorControls>(), c);
    }
}
