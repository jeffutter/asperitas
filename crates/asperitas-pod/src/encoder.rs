//! Pod control-surface driver — rotary encoder, click switch, and pushbuttons.
//!
//! All five digital inputs use a uniform polling approach (no EXTI interrupts).
//! Rationale:
//! - AC #5 requires a single approach across all inputs
//! - Software quadrature decoding needs cross-edge state tracking that doesn't
//!   split cleanly across independent interrupt handlers
//! - TASK-018.02 already polls knobs; one control-surface task is simpler than
//!   mixing polled and interrupt paths
//! - PD11 (encoder A) has no timer alternate function, ruling out hardware QEI
//!
//! ### Debounce strategy
//!
//! Encoder rotation: inherent self-debouncing via Gray-code LUT. Bounce states
//! (both bits changing simultaneously: 00→11 or 01→10) map to delta = 0 in the
//! transition table, so contact bounce does not produce spurious increments.
//!
//! ### Transitions versus detents
//!
//! The Pod's encoder is detented at every **fourth** quadrature state, so one
//! physical click walks all four Gray states and yields four ±1 transitions.
//! [`EncoderDecoder`] therefore accumulates raw transitions ("quarter-steps")
//! and converts them to whole detents only at drain time, in
//! [`EncoderDecoder::drain_detents`], which carries the sub-detent remainder
//! forward. `ControlEvent::EncoderDelta` is in detents, not transitions.
//!
//! The conversion cannot recover a transition that was never observed: when two
//! edges arrive inside one poll window the decoder sees a both-bits change and
//! counts nothing, so that click contributes fewer than four quarter-steps and
//! its detent is *deferred* rather than lost. Widening the poll interval makes
//! that aliasing worse, which is why this sits on the ~1 kHz precondition below
//! (TASK-025/TASK-026); the measured numbers are in
//! `docs/reference/daisy-pod.md` § "Encoder detent ratio".
//!
//! Buttons / click switch: consecutive-stable-readings debouncer. An edge is
//! emitted only after DEBOUNCE_TICKS consecutive readings agree. At 1 kHz poll
//! rate with DEBOUNCE_TICKS = 5, this gives ~5 ms debounce — sufficient for
//! mechanical switch bounce (typically 1–10 ms).
//!
//! This assumes each `poll()` call is spaced ~1 ms apart in wall-clock time.
//! Callers must drive `poll()` from a fixed-period scheduler,
//! `embassy_time::Ticker::every()`, and bound Ticker catch-up bursts with
//! [`crate::ticker_guard::should_reset`] (see that module for the overrun
//! policy) so a stalled executor cannot compress DEBOUNCE_TICKS readings
//! into a back-to-back burst.

// ---------------------------------------------------------------------------
// Algorithmic logic — host-testable, no hardware dependency
// ---------------------------------------------------------------------------

/// Number of consecutive stable readings required before emitting an edge.
///
/// At 1 kHz polling interval, this gives ~5 ms debounce. Mechanical switches
/// typically bounce for 1–10 ms, so 5 ms covers the majority case.
const DEBOUNCE_TICKS: u8 = 5;

/// Edge event emitted by a debounced switch.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Edge {
    /// Switch transitioned from open to closed (pressed).
    Press,
    /// Switch transitioned from closed to open (released).
    Release,
}

/// Decoded events from the control surface.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ControlEvent {
    /// Encoder rotated by signed detent increment (clockwise positive).
    EncoderDelta(i8),
    /// Encoder push switch pressed.
    ClickPress,
    /// Encoder push switch released.
    ClickRelease,
    /// Button 1 pressed.
    Button1Press,
    /// Button 1 released.
    Button1Release,
    /// Button 2 pressed.
    Button2Press,
    /// Button 2 released.
    Button2Release,
}

/// Software quadrature decoder using a Gray-code transition table.
///
/// Rotary encoders output Gray code on two channels (A and B). As the shaft
/// rotates one detent, exactly one channel changes state at a time:
///
/// | State | Meaning          |
/// |-------|------------------|
/// | 00    | Reference        |
/// | 01    | +1 detent        |
/// | 10    | -1 detent        |
/// | 11    | Reference        |
///
/// The transition table maps each (previous_state, current_state) pair to a
/// signed delta. Bounce states where both bits change simultaneously (00→11,
/// 01→10) map to delta = 0, providing inherent self-debouncing.
///
/// The LUT is indexed as `[previous_state << 2 | current_state]`, giving a
/// flat 16-entry array.
pub struct EncoderDecoder {
    previous_state: u8,
    /// Raw quadrature transitions accumulated since the last detent emission.
    ///
    /// `i16` rather than `i8` so that a caller which stops draining saturates
    /// far enough away to be noticed instead of wrapping the sign of a rotation
    /// within a few hundred polls.
    quarter_steps: i16,
}

/// Quadrature transitions per physical detent on the Pod's encoder.
///
/// The Pod's encoder is detented at every **fourth** quadrature state: measured
/// 2026-08-08, ten deliberate detents produced a net ±40 LUT counts. See
/// `docs/reference/daisy-pod.md` § "Encoder detent ratio".
const QUARTER_STEPS_PER_DETENT: i16 = 4;

/// 16-entry Gray-code transition table for quadrature decoding.
///
/// Indexed as `[previous_state << 2 | current_state]`. Each entry is the
/// signed delta for that transition. Bounce transitions (both bits changing)
/// produce 0, which filters contact bounce without explicit timing logic.
///
/// Transitions:
/// - 00 → 01: +1 (clockwise)
/// - 01 → 11: +1 (clockwise)
/// - 11 → 10: +1 (clockwise)
/// - 10 → 00: +1 (clockwise)
/// - 00 → 10: -1 (counter-clockwise)
/// - 10 → 01: -1 (counter-clockwise)
/// - 01 → 00: -1 (counter-clockwise)
/// - 11 → 01: -1 (counter-clockwise)
/// - same-state: 0 (no movement)
/// - both-bits-change: 0 (bounce filtered)
const ENCODER_LUT: [i8; 16] = [
    /* prev=00 */ 0, 1, -1, 0, // curr=00, 01, 10, 11
    /* prev=01 */ -1, 0, 0, 1, // curr=00, 01, 10, 11
    /* prev=10 */ 1, 0, 0, -1, // curr=00, 01, 10, 11
    /* prev=11 */ 0, -1, 1, 0, // curr=00, 01, 10, 11
];

impl EncoderDecoder {
    pub fn new() -> Self {
        Self {
            previous_state: 0,
            quarter_steps: 0,
        }
    }

    /// Update state with current pin readings and accumulate one quarter-step.
    ///
    /// Takes the raw 2-bit state (A as bit 1, B as bit 0) and looks up the
    /// transition in the Gray-code LUT. Only the low 2 bits of `current_state`
    /// are significant; higher bits are masked off so any input is in range
    /// for the LUT lookup. Returns nothing; transitions accumulate internally
    /// until converted to detents by [`Self::drain_detents`].
    pub fn update(&mut self, current_state: u8) {
        let current_state = current_state & 0b11;
        let idx = (self.previous_state << 2) | current_state;
        let delta = ENCODER_LUT[idx as usize];
        self.quarter_steps = self.quarter_steps.saturating_add(i16::from(delta));
        self.previous_state = current_state;
    }

    /// Drain whole detents accumulated since the last call, keeping the
    /// leftover quarter-steps for the next one.
    ///
    /// Two properties here are load-bearing and a caller cannot reconstruct
    /// them from the returned value:
    ///
    /// - **The remainder outlives the drain.** A click whose transitions were
    ///   split across polls, or partly aliased away, registers fewer than four
    ///   transitions; those missing counts stay in the accumulator and make the
    ///   *next* detent arrive sooner instead of being discarded. This is why the
    ///   conversion cannot live at a call site as a per-poll `delta / 4`: every
    ///   sub-detent residue that would throw away is exactly a fraction of a
    ///   click, and they accumulate into real clicks.
    /// - **Truncation is toward zero**, which is plain Rust `/`. Do not "tidy"
    ///   this into `div_euclid`/`rem_euclid`: `(-3).div_euclid(4)` is `-1`, which
    ///   would report a detent whose four transitions never arrived. Truncating
    ///   toward zero means a detent is never fabricated and the retained
    ///   remainder keeps the sign of the rotation.
    ///
    /// The clamp is unreachable while every poll drains (as
    /// [`ControlSurface::poll`] does); it exists so the `as i8` narrowing is a
    /// decision rather than luck should a caller stop draining. See
    /// `drain_clamps_rather_than_wrapping_when_undrained`.
    pub fn drain_detents(&mut self) -> i8 {
        let detents =
            (self.quarter_steps / QUARTER_STEPS_PER_DETENT).clamp(i8::MIN as i16, i8::MAX as i16);
        self.quarter_steps -= detents * QUARTER_STEPS_PER_DETENT;
        detents as i8
    }
}

impl Default for EncoderDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Debounced switch tracker.
///
/// Counts consecutive readings at the same level. When the count reaches
/// `DEBOUNCE_TICKS` and the level differs from the confirmed level, emits
/// an edge and updates the confirmed level. Any disagreement resets the
/// counter, filtering mechanical contact bounce.
pub struct DebouncedSwitch {
    confirmed_level: bool,
    current_level: bool,
    consecutive: u8,
}

impl DebouncedSwitch {
    pub fn new(initial_level: bool) -> Self {
        Self {
            confirmed_level: initial_level,
            current_level: initial_level,
            consecutive: 0,
        }
    }

    /// Update with a new reading. Returns `Some(Edge)` if a debounced edge occurred.
    ///
    /// Accumulates consecutive readings at the same level. When the count reaches
    /// DEBOUNCE_TICKS and the level differs from confirmed, emits an edge.
    /// Any level change resets the counter.
    pub fn update(&mut self, reading: bool) -> Option<Edge> {
        if reading == self.current_level {
            self.consecutive = self.consecutive.saturating_add(1);
            if self.consecutive >= DEBOUNCE_TICKS && self.current_level != self.confirmed_level {
                let edge = if self.current_level {
                    Edge::Press
                } else {
                    Edge::Release
                };
                self.confirmed_level = self.current_level;
                return Some(edge);
            }
        } else {
            self.current_level = reading;
            self.consecutive = 1;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Hardware driver — requires embassy-stm32 (pod-hw feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "pod-hw")]
mod hw {
    use super::{DebouncedSwitch, EncoderDecoder};
    use embassy_stm32::{
        self as hal,
        gpio::{Input, Pull},
        Peri,
    };

    use super::ControlEvent;

    /// Unified Pod control-surface driver.
    ///
    /// Owns all five digital inputs (encoder A/B, encoder click, button 1, button 2)
    /// and provides decoded events via polling. Callers invoke `poll()` from an
    /// embassy task at ~1 kHz, collecting events into a buffer or acting immediately.
    ///
    /// The pod uses mechanical switches pulled to ground when pressed. Pins are
    /// configured as inputs with pull-up resistors, so pressed = `Low`.
    pub struct ControlSurface {
        enc_a: Input<'static>,
        enc_b: Input<'static>,
        click: Input<'static>,
        button1: Input<'static>,
        button2: Input<'static>,
        encoder: EncoderDecoder,
        click_switch: DebouncedSwitch,
        button1_switch: DebouncedSwitch,
        button2_switch: DebouncedSwitch,
    }

    impl ControlSurface {
        /// Create a new control-surface driver from the five Pod pins.
        ///
        /// Pins are configured as inputs with internal pull-up resistors.
        /// The Pod connects switches to ground when pressed, so pressed = Low.
        pub fn new(
            enc_a: impl Into<Peri<'static, hal::peripherals::PD11>>,
            enc_b: impl Into<Peri<'static, hal::peripherals::PA0>>,
            click: impl Into<Peri<'static, hal::peripherals::PB6>>,
            button1: impl Into<Peri<'static, hal::peripherals::PG9>>,
            button2: impl Into<Peri<'static, hal::peripherals::PA2>>,
        ) -> Self {
            let enc_a = Input::new(enc_a.into(), Pull::Up);
            let enc_b = Input::new(enc_b.into(), Pull::Up);
            let click = Input::new(click.into(), Pull::Up);
            let button1 = Input::new(button1.into(), Pull::Up);
            let button2 = Input::new(button2.into(), Pull::Up);

            // Initial levels: unpressed switches read High with pull-up.
            // We invert to "pressed" semantics: true = pressed (Low), false = released (High).
            let initial_pressed = false;

            Self {
                encoder: EncoderDecoder::new(),
                click_switch: DebouncedSwitch::new(initial_pressed),
                button1_switch: DebouncedSwitch::new(initial_pressed),
                button2_switch: DebouncedSwitch::new(initial_pressed),
                enc_a,
                enc_b,
                click,
                button1,
                button2,
            }
        }

        /// Poll all five inputs and yield decoded events via the callback.
        ///
        /// Samples encoder A/B for quadrature state, and all three switches
        /// for debounced edges. Each event is passed to the `events` callback.
        ///
        /// Call from a control-surface task at ~1 kHz, scheduled with
        /// `embassy_time::Ticker::every()` and bounded by
        /// [`crate::ticker_guard::should_reset`] (see module-level debounce
        /// documentation for why this matters). Do NOT call from the audio
        /// callback — GPIO reads are blocking and would disrupt audio timing.
        pub fn poll(&mut self, mut events: impl FnMut(super::ControlEvent)) {
            // Read encoder state: A is bit 1, B is bit 0.
            // Pin is inverted (pull-up, active-low): Low = active.
            let a_active = !self.enc_a.is_high();
            let b_active = !self.enc_b.is_high();
            let encoder_state = (if a_active { 2 } else { 0 }) | (if b_active { 1 } else { 0 });
            self.encoder.update(encoder_state);

            // Drain whole detents. Any sub-detent residue stays inside the
            // decoder, which is what makes calling this every poll correct
            // rather than lossy. Only report non-zero detents to avoid flooding
            // the event stream with no-op samples.
            let detents = self.encoder.drain_detents();
            if detents != 0 {
                events(ControlEvent::EncoderDelta(detents));
            }

            // Sample switches. Active-low: Low = pressed = true.
            let click_pressed = !self.click.is_high();
            let btn1_pressed = !self.button1.is_high();
            let btn2_pressed = !self.button2.is_high();

            if let Some(edge) = self.click_switch.update(click_pressed) {
                events(match edge {
                    super::Edge::Press => ControlEvent::ClickPress,
                    super::Edge::Release => ControlEvent::ClickRelease,
                });
            }

            if let Some(edge) = self.button1_switch.update(btn1_pressed) {
                events(match edge {
                    super::Edge::Press => ControlEvent::Button1Press,
                    super::Edge::Release => ControlEvent::Button1Release,
                });
            }

            if let Some(edge) = self.button2_switch.update(btn2_pressed) {
                events(match edge {
                    super::Edge::Press => ControlEvent::Button2Press,
                    super::Edge::Release => ControlEvent::Button2Release,
                });
            }
        }
    }
}

#[cfg(feature = "pod-hw")]
pub use hw::ControlSurface;

// ---------------------------------------------------------------------------
// Tests — host-testable algorithmic logic (no hardware feature needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── EncoderDecoder LUT tests ──────────────────────────────────────

    #[test]
    fn one_transition_is_a_quarter_step_and_does_not_emit() {
        // 00 → 01 is one of the four transitions that make up a physical click.
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, 1);
    }

    #[test]
    fn one_counter_clockwise_transition_emits_nothing() {
        // 00 → 10 = -1 quarter-step
        let mut dec = EncoderDecoder::new();
        dec.update(2); // 00 → 10
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, -1);
    }

    #[test]
    fn four_transitions_clockwise_emit_one_detent() {
        // 00 → 01 → 11 → 10 → 00: one physical detent.
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01 (+1)
        dec.update(3); // 01 → 11 (+1)
        dec.update(2); // 11 → 10 (+1)
        dec.update(0); // 10 → 00 (+1)
        assert_eq!(dec.drain_detents(), 1);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn four_transitions_counter_clockwise_emit_one_detent() {
        // 00 → 10 → 11 → 01 → 00: one physical detent, counter-clockwise.
        let mut dec = EncoderDecoder::new();
        dec.update(2); // 00 → 10 (-1)
        dec.update(3); // 10 → 11 (-1)
        dec.update(1); // 11 → 01 (-1)
        dec.update(0); // 01 → 00 (-1)
        assert_eq!(dec.drain_detents(), -1);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn bounce_both_bits_00_to_11_adds_no_quarter_step() {
        // Both bits changing simultaneously = bounce, should produce 0
        let mut dec = EncoderDecoder::new();
        dec.update(3); // 00 → 11 (bounce)
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn bounce_both_bits_01_to_10_adds_no_quarter_step() {
        let mut dec = EncoderDecoder::new();
        dec.previous_state = 1;
        dec.update(2); // 01 → 10 (bounce)
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn no_movement_yields_zero() {
        let mut dec = EncoderDecoder::new();
        dec.update(0); // 00 → 00 (no movement)
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn mixed_rotation_and_bounce_filters_bounce_and_retains_residue() {
        // Clockwise step followed by bounce: the bounce adds nothing, and the
        // net sub-detent residue is retained rather than reported as a detent.
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01 (+1)
        dec.update(0); // 01 → 00 (-1, back)
        dec.update(3); // 00 → 11 (bounce = 0)
        dec.update(1); // 11 → 01 (-1)
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, -1);
    }

    #[test]
    fn all_lut_entries_valid() {
        // Verify all 16 LUT entries are valid values (-1, 0, or +1)
        for &entry in ENCODER_LUT.iter() {
            assert!(
                entry == -1 || entry == 0 || entry == 1,
                "invalid LUT entry: {}",
                entry
            );
        }
    }

    #[test]
    fn out_of_range_state_is_masked_not_indexed_out_of_bounds() {
        // Only the low 2 bits are significant; a caller passing stray high
        // bits must not panic on an out-of-bounds LUT index.
        let mut dec = EncoderDecoder::new();
        dec.update(0b1101); // masked to 0b01, same as update(1)
        assert_eq!(dec.quarter_steps, 1);
        assert_eq!(dec.drain_detents(), 0);
    }

    #[test]
    fn lut_symmetry_clockwise_vs_counter() {
        // For every transition A→B with delta D, B→A should have delta -D
        for prev in 0..4u8 {
            for curr in 0..4u8 {
                let fwd_idx = ((prev << 2) | curr) as usize;
                let rev_idx = ((curr << 2) | prev) as usize;
                assert_eq!(
                    ENCODER_LUT[fwd_idx], -ENCODER_LUT[rev_idx],
                    "asymmetry at prev={} curr={}",
                    prev, curr
                );
            }
        }
    }

    // ── Quarter-step → detent conversion ──────────────────────────────

    #[test]
    fn remainder_carries_across_drains() {
        // AC #2 verbatim: three transitions, drained, emit nothing but are not
        // lost; the fourth completes the detent.
        let mut dec = EncoderDecoder::new();
        for state in [1u8, 3, 2] {
            dec.update(state);
            assert_eq!(dec.drain_detents(), 0, "a partial detent must not emit");
        }
        assert_eq!(dec.quarter_steps, 3);
        dec.update(0); // 10 → 00 completes the cycle
        assert_eq!(dec.drain_detents(), 1);
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn truncation_is_toward_zero_not_toward_negative_infinity() {
        // The trap: (-3).div_euclid(4) == -1, which would report a detent whose
        // transitions never arrived. Plain `/` truncates toward zero.
        for partial in [-1i16, -2, -3] {
            let mut dec = EncoderDecoder::new();
            for _ in 0..partial.abs() {
                // 00 → 10 → 11 → 01 : counter-clockwise quarter-steps.
                dec.update(match dec.previous_state {
                    0 => 2,
                    2 => 3,
                    3 => 1,
                    s => unreachable!("unexpected state {s}"),
                });
            }
            assert_eq!(dec.quarter_steps, partial);
            assert_eq!(
                dec.drain_detents(),
                0,
                "{partial} quarter-steps fabricated a detent"
            );
        }
    }

    #[test]
    fn reversal_cancels_retained_remainder() {
        // Three clockwise quarter-steps, then four counter-clockwise: the shaft
        // never travelled a full detent net, so nothing is emitted in either
        // direction and the retained remainder flips sign with it. A per-poll
        // divide gets this backwards both ways — it drops the +3, then reports
        // the -4 as a detent that was partly undone.
        let mut dec = EncoderDecoder::new();
        for state in [1u8, 3, 2] {
            dec.update(state);
        }
        assert_eq!(dec.drain_detents(), 0);
        // Continue counter-clockwise from state 10: 10 → 11 → 01 → 00 → 10.
        for state in [3u8, 1, 0, 2] {
            dec.update(state);
        }
        assert_eq!(dec.drain_detents(), 0);
        assert_eq!(dec.quarter_steps, -1);
    }

    #[test]
    fn drain_clamps_rather_than_wrapping_when_undrained() {
        // Defensive: ControlSurface::poll drains every call, so this path is
        // unreachable today. It pins the `as i8` narrowing in drain_detents().
        let mut dec = EncoderDecoder::new();
        for _ in 0..800 {
            // Step clockwise through the Gray cycle one quarter-step at a time.
            let next = match dec.previous_state {
                0 => 1,
                1 => 3,
                3 => 2,
                2 => 0,
                s => unreachable!("unexpected state {s}"),
            };
            dec.update(next);
        }
        assert_eq!(dec.quarter_steps, 800);
        assert_eq!(dec.drain_detents(), 127, "overflow must clamp, not wrap");
        assert_eq!(dec.drain_detents(), 73, "clamped drain keeps the remainder");
        assert_eq!(dec.quarter_steps, 0);
    }

    #[test]
    fn held_still_emits_nothing_on_repeated_drains() {
        // A static input must never accumulate drift.
        let mut dec = EncoderDecoder::new();
        dec.previous_state = 2; // already resting at this state, so it counts nothing
        for _ in 0..1000 {
            dec.update(2);
            assert_eq!(dec.drain_detents(), 0);
        }
        assert_eq!(dec.quarter_steps, 0);
    }

    // ── Reconstructed hardware capture (TASK-024 AC #3) ───────────────
    //
    // These walks are RECONSTRUCTIONS, not raw captures. No raw per-transition
    // capture exists anywhere: ~/podtest.log is gone from disk and
    // firmware/src/bin/podtest.rs logs only the decoded sum, never the 2-bit
    // state. What survives is aggregate — the per-detent cluster sizes recorded
    // in TASK-024's description, measured 2026-08-08 at ~625 Hz polling:
    //
    //   clockwise:        [4, 4, 3, 4, 4, 3, 4, 4, 4, 4, 4]  (stated net +40)
    //   counter-clockwise: [4, 4, 3, 4, 4, 4, 3, 4, 3, 4]    (stated net -38)
    //
    // Neither array sums to its stated net (+42 vs +40, -37 vs -38), which is
    // why these tests assert the carry invariant rather than those totals. Raw
    // captures arrive with TASK-029.01 / TASK-029.

    /// Recorded cluster sizes: counted LUT transitions per physical click.
    const CLOCKWISE_CLUSTERS: [usize; 11] = [4, 4, 3, 4, 4, 3, 4, 4, 4, 4, 4];
    const COUNTER_CLOCKWISE_CLUSTERS: [usize; 10] = [4, 4, 3, 4, 4, 4, 3, 4, 3, 4];

    /// Gray-code state order for each rotation direction, starting at 00.
    const CLOCKWISE_ORDER: [u8; 4] = [0, 1, 3, 2];
    const COUNTER_CLOCKWISE_ORDER: [u8; 4] = [0, 2, 3, 1];

    /// Result of replaying a reconstructed run through the decoder.
    struct Replay {
        /// Total detents emitted across all drains.
        detents: i32,
        /// Quarter-steps left in the accumulator at the end.
        residue: i32,
        /// Quarter-steps the walk actually contributed, summed from the LUT.
        counted: i32,
    }

    /// Build the polled states for one reconstructed click.
    ///
    /// `counted` is what the capture recorded for that click. A full click is
    /// `counted` in-order steps. A short click additionally carries one
    /// both-bits-change observation standing for the transition that was
    /// aliased away when two edges landed inside one poll window, so the
    /// decoder records exactly `counted` quarter-steps for it.
    ///
    /// `diagonal_at` picks which of the click's observations is the aliased
    /// one; the aggregate record cannot say where it fell, so callers sweep it.
    ///
    /// Returns the observed states packed into the front of the array, how many
    /// there are (at most four per click), and the state the click ends in.
    fn reconstructed_click_walk(
        start: u8,
        counted: usize,
        clockwise: bool,
        diagonal_at: usize,
    ) -> ([u8; QUARTER_STEPS_PER_DETENT as usize], usize, u8) {
        let order = if clockwise {
            CLOCKWISE_ORDER
        } else {
            COUNTER_CLOCKWISE_ORDER
        };
        let aliased = counted < QUARTER_STEPS_PER_DETENT as usize;
        let observations = if aliased { counted + 1 } else { counted };
        let mut walk = [0u8; QUARTER_STEPS_PER_DETENT as usize];
        let mut state = start;
        for (i, slot) in walk.iter_mut().enumerate().take(observations) {
            state = if aliased && i == diagonal_at {
                state ^ 0b11 // both bits change: the LUT counts this as 0
            } else {
                let at = order.iter().position(|&s| s == state).expect("in cycle");
                order[(at + 1) % 4]
            };
            *slot = state;
        }
        (walk, observations, state)
    }

    /// Replay reconstructed clicks through the decoder, draining every
    /// `drain_every` observations (`usize::MAX` drains once at the very end).
    ///
    /// This drives the decoder and reads its answer; it never computes one.
    fn replay_captured_run(
        clusters: &[usize],
        clockwise: bool,
        diagonal_at: usize,
        drain_every: usize,
    ) -> Replay {
        let mut decoder = EncoderDecoder::new();
        let mut state = 0u8;
        let mut previous = 0u8;
        let mut detents = 0i32;
        let mut counted = 0i32;
        let mut since_drain = 0usize;

        for &cluster in clusters {
            let (walk, observations, end) =
                reconstructed_click_walk(state, cluster, clockwise, diagonal_at);
            state = end;
            for &observed in walk.iter().take(observations) {
                // Fixture check, not a prediction: the LUT is unchanged by this
                // ticket, so counting through it confirms the reconstructed
                // walk really registers the transitions the capture recorded.
                counted += ENCODER_LUT[((previous << 2) | observed) as usize] as i32;
                previous = observed;

                decoder.update(observed);
                since_drain += 1;
                if since_drain == drain_every {
                    detents += decoder.drain_detents() as i32;
                    since_drain = 0;
                }
            }
        }
        if since_drain > 0 {
            detents += decoder.drain_detents() as i32;
        }

        Replay {
            detents,
            residue: i32::from(decoder.quarter_steps),
            counted,
        }
    }

    /// Assert the two invariants AC #3 asks of every reconstructed run: nothing
    /// is silently discarded, and no detent is fabricated.
    fn assert_carry_invariants(run: &Replay, physical_clicks: usize, clockwise: bool) {
        assert_eq!(
            run.detents * i32::from(QUARTER_STEPS_PER_DETENT) + run.residue,
            run.counted,
            "detents*4 + residue must equal the counted quarter-steps"
        );
        assert!(
            run.detents.abs() <= physical_clicks as i32,
            "emitted {} detents for {} physical clicks",
            run.detents,
            physical_clicks
        );
        if clockwise {
            assert!(
                run.detents >= 0 && run.residue >= 0,
                "sign followed direction"
            );
        } else {
            assert!(
                run.detents <= 0 && run.residue <= 0,
                "sign followed direction"
            );
        }
    }

    /// Which positions inside a short cluster the aliased transition could have
    /// fallen — the aggregate record cannot say, so every test sweeps them.
    const ALIAS_POSITIONS: [usize; 4] = [0, 1, 2, 3];

    #[test]
    fn clockwise_captured_run_reports_ten_detents_with_residue_retained() {
        // The recorded array carries eleven clusters for what the capture calls
        // ten clockwise detents; the extra one is part of why it sums to +42
        // against a stated net of +40. Eleven is what the decoder was actually
        // shown, so that is what the no-fabrication bound is checked against.
        for diagonal_at in ALIAS_POSITIONS {
            let run = replay_captured_run(&CLOCKWISE_CLUSTERS, true, diagonal_at, usize::MAX);
            assert_eq!(run.counted, 42, "fixture must register +42 transitions");
            assert_eq!(run.detents, 10);
            assert_eq!(run.residue, 2);
            assert_carry_invariants(&run, CLOCKWISE_CLUSTERS.len(), true);
        }
    }

    #[test]
    fn ccw_captured_run_reports_nine_of_ten_with_residue_retained() {
        // Ten physical counter-clockwise clicks, nine reported. The tenth is
        // deferred by the three aliased transitions, and its quarter-steps stay
        // in the accumulator. Pinning nine is deliberate: it documents the limit
        // of this fix. Recovering the aliased counts (direction-guarded diagonal
        // recovery, or the rest-state machine) must update this on purpose — see
        // TASK-029 AC #4 for filing the residual if it reproduces at ~1 kHz.
        for diagonal_at in ALIAS_POSITIONS {
            let run =
                replay_captured_run(&COUNTER_CLOCKWISE_CLUSTERS, false, diagonal_at, usize::MAX);
            assert_eq!(run.counted, -37, "fixture must register -37 transitions");
            assert_eq!(run.detents, -9);
            assert_eq!(run.residue, -1);
            assert_carry_invariants(&run, COUNTER_CLOCKWISE_CLUSTERS.len(), false);
        }
    }

    #[test]
    fn captured_run_counts_do_not_depend_on_drain_cadence() {
        // Draining every poll (the real shape, ~1 observation per poll during a
        // click), every four observations, and once at the end must agree — the
        // remainder lives in the decoder, not at the call site.
        for clockwise in [true, false] {
            let clusters = if clockwise {
                &CLOCKWISE_CLUSTERS[..]
            } else {
                &COUNTER_CLOCKWISE_CLUSTERS[..]
            };
            for diagonal_at in ALIAS_POSITIONS {
                let reference = replay_captured_run(clusters, clockwise, diagonal_at, usize::MAX);
                for cadence in [1usize, 2, 4, 7] {
                    let run = replay_captured_run(clusters, clockwise, diagonal_at, cadence);
                    assert_eq!(
                        (run.detents, run.residue, run.counted),
                        (reference.detents, reference.residue, reference.counted),
                        "cadence {cadence} changed the result ({clockwise}, alias at {diagonal_at})"
                    );
                }
            }
        }
    }

    // ── DebouncedSwitch tests ─────────────────────────────────────────

    #[test]
    fn stable_press_emits_single_edge() {
        let mut sw = DebouncedSwitch::new(false);
        let mut edges: [Option<Edge>; 5] = Default::default();
        let mut count = 0;
        for _ in 0..DEBOUNCE_TICKS {
            if let Some(e) = sw.update(true) {
                edges[count] = Some(e);
                count += 1;
            }
        }
        assert_eq!(count, 1);
        assert_eq!(edges[0], Some(Edge::Press));
    }

    #[test]
    fn single_bounce_pulse_does_not_emit_edge() {
        let mut sw = DebouncedSwitch::new(false);
        assert!(sw.update(true).is_none());
        assert!(sw.update(false).is_none());
    }

    #[test]
    fn release_after_stable_period_emits_release() {
        let mut sw = DebouncedSwitch::new(false);
        // First press
        for _ in 0..DEBOUNCE_TICKS {
            sw.update(true);
        }
        // Then release
        let mut edges: [Option<Edge>; 5] = Default::default();
        let mut count = 0;
        for _ in 0..DEBOUNCE_TICKS {
            if let Some(e) = sw.update(false) {
                edges[count] = Some(e);
                count += 1;
            }
        }
        assert_eq!(count, 1);
        assert_eq!(edges[0], Some(Edge::Release));
    }

    #[test]
    fn rapid_bounce_sequence_no_false_edges() {
        let mut sw = DebouncedSwitch::new(false);
        // Alternating true/false rapidly — counter resets each time
        for _ in 0..(DEBOUNCE_TICKS * 3) {
            assert!(sw.update(true).is_none());
            assert!(sw.update(false).is_none());
        }
    }

    #[test]
    fn stable_state_never_emits_edge() {
        // Stable false should never emit an edge
        let mut sw = DebouncedSwitch::new(false);
        for _ in 0..(DEBOUNCE_TICKS * 5) {
            assert!(sw.update(false).is_none());
        }
    }

    #[test]
    fn long_held_stable_reading_does_not_overflow_consecutive_counter() {
        // A switch held pressed well past 255 polls (u8::MAX) must not panic
        // on arithmetic overflow — a physical hold can outlast that easily
        // at a 1 kHz poll rate.
        let mut sw = DebouncedSwitch::new(false);
        for i in 0..1000u32 {
            let edge = sw.update(true);
            if i < (DEBOUNCE_TICKS - 1) as u32 {
                assert_eq!(edge, None);
            } else {
                assert_eq!(
                    edge,
                    if i == (DEBOUNCE_TICKS - 1) as u32 {
                        Some(Edge::Press)
                    } else {
                        None
                    }
                );
            }
        }
    }

    #[test]
    fn press_then_release_then_repress_no_double_count() {
        let mut sw = DebouncedSwitch::new(false);
        let mut edges: [Edge; 3] = [Edge::Press; 3]; // placeholder
        let mut count = 0i32;

        // Press
        for _ in 0..DEBOUNCE_TICKS {
            if let Some(e) = sw.update(true) {
                edges[count as usize] = e;
                count += 1;
            }
        }

        // Release
        for _ in 0..DEBOUNCE_TICKS {
            if let Some(e) = sw.update(false) {
                edges[count as usize] = e;
                count += 1;
            }
        }

        // Re-press
        for _ in 0..DEBOUNCE_TICKS {
            if let Some(e) = sw.update(true) {
                edges[count as usize] = e;
                count += 1;
            }
        }

        // Should be exactly: Press, Release, Press (3 edges, no doubles)
        assert_eq!(count, 3);
        assert_eq!(edges[0], Edge::Press);
        assert_eq!(edges[1], Edge::Release);
        assert_eq!(edges[2], Edge::Press);
    }

    #[test]
    fn burst_of_identical_readings_emits_edge_without_wall_clock() {
        // Demonstrates why call-site overrun detection is necessary:
        // DebouncedSwitch has no wall-clock awareness — if DEBOUNCE_TICKS
        // readings arrive in rapid succession (as happens when Ticker replays
        // a backlog after an executor stall), it emits an edge despite zero
        // real time elapsing. The call-site guard (TASK-026) prevents this
        // by resetting the ticker on overrun so at most one tick fires.
        let mut sw = DebouncedSwitch::new(false);
        // Feed DEBOUNCE_TICKS identical "pressed" readings back-to-back.
        // In a burst scenario, these represent ~0 ms of real time,
        // not the ~5 ms debounce window.
        let mut edge_count = 0u32;
        for _ in 0..DEBOUNCE_TICKS {
            if sw.update(true).is_some() {
                edge_count += 1;
            }
        }
        // An edge was emitted based purely on call count with no temporal
        // separation — exactly the failure mode TASK-026's call-site guard
        // prevents in production.
        assert_eq!(edge_count, 1, "burst input produces spurious edge");
    }
}
