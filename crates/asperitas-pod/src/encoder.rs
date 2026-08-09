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
//! transition table, so contact bounce produces spurious detents automatically.
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
    accumulated_delta: i8,
}

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
            accumulated_delta: 0,
        }
    }

    /// Update state with current pin readings and accumulate delta.
    ///
    /// Takes the raw 2-bit state (A as bit 1, B as bit 0) and looks up the
    /// transition in the Gray-code LUT. Only the low 2 bits of `current_state`
    /// are significant; higher bits are masked off so any input is in range
    /// for the LUT lookup. Returns nothing; delta accumulates internally
    /// until drained via `drain_delta()`.
    pub fn update(&mut self, current_state: u8) {
        let current_state = current_state & 0b11;
        let idx = (self.previous_state << 2) | current_state;
        let delta = ENCODER_LUT[idx as usize];
        self.accumulated_delta += delta;
        self.previous_state = current_state;
    }

    /// Drain the accumulated delta, resetting to zero.
    ///
    /// Call after processing to get the net rotation since last drain.
    pub fn drain_delta(&mut self) -> i8 {
        let delta = self.accumulated_delta;
        self.accumulated_delta = 0;
        delta
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

            // Drain accumulated encoder delta. Only report non-zero deltas
            // to avoid flooding the event stream with no-op samples.
            let encoder_delta = self.encoder.drain_delta();
            if encoder_delta != 0 {
                events(ControlEvent::EncoderDelta(encoder_delta));
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
    fn clockwise_one_detent_from_zero() {
        // 00 → 01 = +1
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01
        assert_eq!(dec.drain_delta(), 1);
    }

    #[test]
    fn counter_clockwise_one_detent_from_zero() {
        // 00 → 10 = -1
        let mut dec = EncoderDecoder::new();
        dec.update(2); // 00 → 10
        assert_eq!(dec.drain_delta(), -1);
    }

    #[test]
    fn full_clockwise_cycle() {
        // 00 → 01 → 11 → 10 → 00 = +4
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01 (+1)
        dec.update(3); // 01 → 11 (+1)
        dec.update(2); // 11 → 10 (+1)
        dec.update(0); // 10 → 00 (+1)
        assert_eq!(dec.drain_delta(), 4);
    }

    #[test]
    fn full_counter_clockwise_cycle() {
        // 00 → 10 → 11 → 01 → 00 = -4
        let mut dec = EncoderDecoder::new();
        dec.update(2); // 00 → 10 (-1)
        dec.update(3); // 10 → 11 (-1)
        dec.update(1); // 11 → 01 (-1)
        dec.update(0); // 01 → 00 (-1)
        assert_eq!(dec.drain_delta(), -4);
    }

    #[test]
    fn bounce_both_bits_00_to_11_yields_zero() {
        // Both bits changing simultaneously = bounce, should produce 0
        let mut dec = EncoderDecoder::new();
        dec.update(3); // 00 → 11 (bounce)
        assert_eq!(dec.drain_delta(), 0);
    }

    #[test]
    fn bounce_both_bits_01_to_10_yields_zero() {
        let mut dec = EncoderDecoder::new();
        dec.previous_state = 1;
        dec.update(2); // 01 → 10 (bounce)
        assert_eq!(dec.drain_delta(), 0);
    }

    #[test]
    fn no_movement_yields_zero() {
        let mut dec = EncoderDecoder::new();
        dec.update(0); // 00 → 00 (no movement)
        assert_eq!(dec.drain_delta(), 0);
    }

    #[test]
    fn mixed_rotation_and_bounce_filters_bounce() {
        // Clockwise step followed by bounce should give correct result
        let mut dec = EncoderDecoder::new();
        dec.update(1); // 00 → 01 (+1)
        dec.update(0); // 01 → 00 (-1, back)
        dec.update(3); // 00 → 11 (bounce = 0)
        dec.update(1); // 11 → 01 (-1)
        assert_eq!(dec.drain_delta(), -1);
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
        assert_eq!(dec.drain_delta(), 1);
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
