/// A stereo audio frame (left, right).
pub type Frame = [f32; 2];

/// Core interface for real-time audio processors.
///
/// Design principles:
/// - **`tick` is the primitive; `process_block` is provided.** Implementers write the simple thing.
/// - **Params are an associated type**, not a bag of floats. Knob-to-parameter mapping belongs
///   to the caller; the DSP owns parameter semantics.
/// - **No allocation, no `Result`.** Clamp, saturate, and make degenerate values ordinary ones.
/// - **Parameter smoothing lives inside processors** so all hosts don't reimplement it.
pub trait Processor {
    /// Parameter set for this processor. Must be cloneable and have a sensible default.
    type Params: Clone + Default;

    /// Set the sample rate. Called before processing begins or when rate changes.
    fn set_sample_rate(&mut self, hz: f32);

    /// Update parameters. Takes a reference so callers can hand in their stored params struct.
    fn set_params(&mut self, params: &Self::Params);

    /// Process a single stereo frame.
    fn tick(&mut self, input: Frame) -> Frame;

    /// Reset internal state to initial conditions. After reset, identical input produces
    /// identical output regardless of prior history.
    fn reset(&mut self);

    /// Process a block of frames. Provided implementation delegates to `tick`.
    /// Override only if you have a genuinely more efficient block-level algorithm.
    ///
    /// If `input` and `output` differ in length, only the overlapping prefix is processed;
    /// this never panics, per the no-`Result`, real-time-safe contract above.
    fn process_block(&mut self, input: &[Frame], output: &mut [Frame]) {
        for (inp, out) in input.iter().zip(output.iter_mut()) {
            *out = self.tick(*inp);
        }
    }
}
