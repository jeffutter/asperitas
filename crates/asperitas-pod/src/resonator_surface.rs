//! The Pod's mapping onto the resonator: which input moves which control, and what the two RGB
//! LEDs show. Pure logic over [`ResonatorControls`], so the translation is host-testable and the
//! firmware only feeds it events and latches its output.
//!
//! - knob 1 is structure, knob 2 is damping (the two knobs fill positions 1 and 2);
//! - an encoder turn moves the root by whole semitones, or by cents while fine-tune is on
//!   ([`ResonatorControls::with_root_offset`] is the stepping rule);
//! - the encoder click toggles fine-tune;
//! - button 1 steps the excitation preset, wrapping after the last;
//! - button 2 is reserved and does nothing.
//!
//! The root, spread and preset positions are the surface's own state; the knobs are passed in each
//! time so a host override can be merged into them upstream of this type.

use crate::encoder::ControlEvent;
use asperitas_dsp::{
    ResonatorControls, EXCITATION_PRESETS, RESONATOR_CONTROL_PARAMS, RESONATOR_KNOB_COUNT,
};
use libm::{log2f, roundf};

/// Position index of the structure control.
const STRUCTURE: usize = 1;
/// Position index of the damping control.
const DAMPING: usize = 2;
/// Position index of the root control.
const ROOT: usize = 0;
/// Position index of the excitation preset control.
const PRESET: usize = 4;

/// An RGB LED state: one flag per channel, `true` lit. Matches `RgbLed::set_channels`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    pub r: bool,
    pub g: bool,
    pub b: bool,
}

const fn rgb(r: bool, g: bool, b: bool) -> Rgb {
    Rgb { r, g, b }
}

/// All channels off.
pub const RGB_OFF: Rgb = rgb(false, false, false);
const RED: Rgb = rgb(true, false, false);
const GREEN: Rgb = rgb(false, true, false);
const BLUE: Rgb = rgb(false, false, true);
const YELLOW: Rgb = rgb(true, true, false);
const CYAN: Rgb = rgb(false, true, true);
const MAGENTA: Rgb = rgb(true, false, true);
const WHITE: Rgb = rgb(true, true, true);

/// LED 1 colour per pitch class of the root, C first. Naturals take the seven lit colours in order
/// (C red, D yellow, E green, F cyan, G blue, A magenta, B white); with seven colours for twelve
/// classes, a sharp shares the colour of the natural above it.
pub const ROOT_COLOURS: [Rgb; 12] = [
    RED,     // C
    YELLOW,  // C#
    YELLOW,  // D
    GREEN,   // D#
    GREEN,   // E
    CYAN,    // F
    BLUE,    // F#
    BLUE,    // G
    MAGENTA, // G#
    MAGENTA, // A
    WHITE,   // A#
    WHITE,   // B
];

/// Pitch class of `root_hz` (0 = C), rounded to the nearest semitone. A non-positive or NaN
/// frequency has no pitch and gives class 0.
fn pitch_class(root_hz: f32) -> usize {
    if root_hz.is_nan() || root_hz <= 0.0 {
        return 0;
    }
    // MIDI note 69 is A4 = 440 Hz; class 0 is C.
    let midi = 69.0 + 12.0 * log2f(root_hz / 440.0);
    (roundf(midi) as i32).rem_euclid(12) as usize
}

/// LED 1 colour for a root frequency: its pitch class through [`ROOT_COLOURS`].
pub fn led_root_colour(root_hz: f32) -> Rgb {
    ROOT_COLOURS[pitch_class(root_hz)]
}

/// Peak at or above which LED 2 lights at all: -40 dBFS.
pub const LEVEL_GREEN_PEAK: f32 = 0.01;
/// Peak at or above which LED 2 turns yellow: -12 dBFS.
pub const LEVEL_YELLOW_PEAK: f32 = 0.251_188_64;
/// Peak at or above which LED 2 turns red: -1 dBFS.
pub const LEVEL_RED_PEAK: f32 = 0.891_250_9;

/// LED 2 colour for an input peak (linear, full scale 1.0): off below [`LEVEL_GREEN_PEAK`], then
/// green, yellow and red at their thresholds. NaN is off.
pub fn led_level(peak: f32) -> Rgb {
    if peak >= LEVEL_RED_PEAK {
        RED
    } else if peak >= LEVEL_YELLOW_PEAK {
        YELLOW
    } else if peak >= LEVEL_GREEN_PEAK {
        GREEN
    } else {
        RGB_OFF
    }
}

/// The Pod's state for the resonator: root, spread and preset positions, and fine-tune.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResonatorSurface {
    controls: ResonatorControls,
    fine: bool,
}

impl ResonatorSurface {
    /// Apply one decoded control event.
    pub fn apply_event(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::EncoderDelta(detents) => {
                let n = i32::from(detents);
                self.controls = if self.fine {
                    self.controls.with_root_offset(0, n)
                } else {
                    self.controls.with_root_offset(n, 0)
                };
            }
            ControlEvent::ClickPress => self.fine = !self.fine,
            ControlEvent::Button1Press => {
                let count = EXCITATION_PRESETS.len();
                let current = roundf(self.controls.value(PRESET)) as usize;
                let next = current.clamp(1, count) % count + 1;
                self.controls.positions[PRESET] =
                    RESONATOR_CONTROL_PARAMS[PRESET].to_normalised(next as f32);
            }
            // Releases carry no meaning, and button 2 is reserved.
            ControlEvent::ClickRelease
            | ControlEvent::Button1Release
            | ControlEvent::Button2Press
            | ControlEvent::Button2Release => {}
        }
    }

    /// All control positions for the two knob readings: structure and damping from the
    /// knobs as given (clamping and NaN handling belong to [`ResonatorControls`]), the rest from
    /// the surface.
    pub fn positions(&self, knobs: [f32; 2]) -> [f32; RESONATOR_KNOB_COUNT] {
        let mut positions = self.controls.positions;
        positions[STRUCTURE] = knobs[0];
        positions[DAMPING] = knobs[1];
        positions
    }

    /// The controls for the two knob readings.
    pub fn controls(&self, knobs: [f32; 2]) -> ResonatorControls {
        ResonatorControls {
            positions: self.positions(knobs),
        }
    }

    /// Whether the encoder currently moves cents instead of semitones.
    pub fn fine(&self) -> bool {
        self.fine
    }

    /// The root frequency in hertz, for LED 1.
    pub fn root_hz(&self) -> f32 {
        self.controls.value(ROOT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOBS: [f32; 2] = [0.3, 0.7];

    fn surface_after(events: &[ControlEvent]) -> ResonatorSurface {
        let mut s = ResonatorSurface::default();
        for &e in events {
            s.apply_event(e);
        }
        s
    }

    fn preset(s: &ResonatorSurface) -> f32 {
        s.controls(KNOBS).values()[PRESET]
    }

    #[test]
    fn default_surface_is_the_default_controls_with_the_knobs_in_place() {
        let d = ResonatorControls::default();
        let c =
            ResonatorSurface::default().controls([d.positions[STRUCTURE], d.positions[DAMPING]]);
        assert_eq!(c, d);
        let v = d.values();
        assert!((v[0] - 110.0).abs() < 1e-3);
        assert_eq!(v[STRUCTURE], 0.0);
        assert_eq!(v[DAMPING], 0.5);
        assert_eq!(v[PRESET], 1.0);
        assert_eq!(v[3], 1.0, "wide spread");
    }

    #[test]
    fn knob_one_is_structure_and_knob_two_is_damping() {
        let c = ResonatorSurface::default().controls([0.25, 0.75]);
        assert_eq!(c.positions[STRUCTURE], 0.25);
        assert_eq!(c.positions[DAMPING], 0.75);
        // A swapped order must be distinguishable.
        assert_ne!(c.positions[STRUCTURE], 0.75);
        assert_ne!(c.positions[DAMPING], 0.25);
    }

    #[test]
    fn out_of_range_and_nan_knobs_reach_the_controls_own_handling() {
        let s = ResonatorSurface::default();
        for bad in [f32::NAN, -1.0, 2.0, f32::INFINITY, f32::NEG_INFINITY] {
            for knobs in [[bad, 0.4], [0.4, bad]] {
                let via_surface = s.controls(knobs);
                let mut direct = ResonatorControls::default();
                direct.positions[STRUCTURE] = knobs[0];
                direct.positions[DAMPING] = knobs[1];
                assert_eq!(via_surface.params(), direct.params());
                assert_eq!(
                    via_surface.values().map(f32::is_nan),
                    [false; RESONATOR_KNOB_COUNT]
                );
            }
        }
    }

    #[test]
    fn encoder_steps_semitones_and_clamps_at_the_range_ends() {
        let s = surface_after(&[ControlEvent::EncoderDelta(12)]);
        assert!((s.root_hz() - 220.0).abs() < 0.01, "{}", s.root_hz());
        let s = surface_after(&[ControlEvent::EncoderDelta(-1)]);
        assert!((s.root_hz() - 110.0 * 2f32.powf(-1.0 / 12.0)).abs() < 0.01);
        let s = surface_after(&[ControlEvent::EncoderDelta(i8::MAX); 4]);
        assert_eq!(s.root_hz(), 2000.0);
        let s = surface_after(&[ControlEvent::EncoderDelta(i8::MIN); 4]);
        assert_eq!(s.root_hz(), 20.0);
    }

    #[test]
    fn click_toggles_fine_tune_and_the_encoder_then_moves_cents() {
        let mut s = surface_after(&[ControlEvent::ClickPress]);
        assert!(s.fine());
        s.apply_event(ControlEvent::EncoderDelta(10));
        let want = 110.0 * 2f32.powf(10.0 / 1200.0);
        assert!((s.root_hz() - want).abs() < 0.01, "{}", s.root_hz());
        s.apply_event(ControlEvent::ClickPress);
        assert!(!s.fine());
        s.apply_event(ControlEvent::EncoderDelta(1));
        let want = want * 2f32.powf(1.0 / 12.0);
        assert!((s.root_hz() - want).abs() < 0.02, "{}", s.root_hz());
    }

    #[test]
    fn button_one_cycles_the_presets_and_wraps() {
        let mut s = ResonatorSurface::default();
        let n = EXCITATION_PRESETS.len();
        for step in 0..=2 * n {
            assert_eq!(preset(&s), (step % n + 1) as f32, "step {step}");
            s.apply_event(ControlEvent::Button1Press);
        }
    }

    #[test]
    fn releases_and_button_two_change_nothing() {
        let base = surface_after(&[
            ControlEvent::EncoderDelta(3),
            ControlEvent::ClickPress,
            ControlEvent::Button1Press,
        ]);
        for e in [
            ControlEvent::ClickRelease,
            ControlEvent::Button1Release,
            ControlEvent::Button2Press,
            ControlEvent::Button2Release,
        ] {
            let mut s = base.clone();
            s.apply_event(e);
            assert_eq!(s, base, "{e:?}");
        }
    }

    #[test]
    fn root_colour_covers_every_pitch_class_and_a2_is_magenta() {
        assert_eq!(led_root_colour(110.0), MAGENTA);
        // C2 = 65.406 Hz and each semitone above it walks the table in order.
        let c2 = 65.406_39_f32;
        for (pc, colour) in ROOT_COLOURS.iter().enumerate() {
            let hz = c2 * 2f32.powf(pc as f32 / 12.0);
            assert_eq!(led_root_colour(hz), *colour, "class {pc}");
        }
        // Wraps by octave and rounds to the nearest semitone.
        assert_eq!(led_root_colour(c2 * 4.0), ROOT_COLOURS[0]);
        assert_eq!(led_root_colour(c2 * 2f32.powf(0.4 / 12.0)), ROOT_COLOURS[0]);
        assert_eq!(led_root_colour(c2 * 2f32.powf(0.6 / 12.0)), ROOT_COLOURS[1]);
        assert_eq!(led_root_colour(f32::NAN), ROOT_COLOURS[0]);
        assert_eq!(led_root_colour(0.0), ROOT_COLOURS[0]);
    }

    #[test]
    fn level_thresholds_at_each_boundary() {
        assert_eq!(led_level(0.0), RGB_OFF);
        assert_eq!(led_level(LEVEL_GREEN_PEAK * 0.99), RGB_OFF);
        assert_eq!(led_level(LEVEL_GREEN_PEAK), GREEN);
        assert_eq!(led_level(LEVEL_YELLOW_PEAK * 0.99), GREEN);
        assert_eq!(led_level(LEVEL_YELLOW_PEAK), YELLOW);
        assert_eq!(led_level(LEVEL_RED_PEAK * 0.99), YELLOW);
        assert_eq!(led_level(LEVEL_RED_PEAK), RED);
        assert_eq!(led_level(1.0), RED);
        assert_eq!(led_level(f32::NAN), RGB_OFF);
        assert_eq!(led_level(-1.0), RGB_OFF);
    }
}
