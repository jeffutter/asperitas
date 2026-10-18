//! Regenerate the stimulus the rig played, from the same processors the firmware runs.

use asperitas_dsp::processor::Processor;
use asperitas_dsp::stimulus::{
    ExponentialSweep, ExponentialSweepParams, PulseTrain, PulseTrainParams, Sine, SineParams,
};

use crate::capture::SAMPLE_RATE_HZ;

/// A stimulus and the parameters the rig was built with.
///
/// `Default` params match the firmware, which builds each generator with `Default::default()`.
#[derive(Clone, Debug)]
pub enum Stimulus {
    Sine(SineParams),
    Sweep(ExponentialSweepParams),
    Pulse(PulseTrainParams),
}

impl Stimulus {
    /// The first `n_samples` of the left lane, the one the firmware drives into the loopback.
    ///
    /// The generator is reset before rendering, as the firmware does when a capture begins, so the
    /// first sample is the stimulus's first sample.
    pub fn render(&self, n_samples: usize) -> Vec<f32> {
        match self {
            Stimulus::Sine(p) => run(Sine::default(), p, n_samples),
            Stimulus::Sweep(p) => run(ExponentialSweep::default(), p, n_samples),
            Stimulus::Pulse(p) => run(PulseTrain::default(), p, n_samples),
        }
    }
}

fn run<P: Processor>(mut gen: P, params: &P::Params, n: usize) -> Vec<f32> {
    gen.set_sample_rate(SAMPLE_RATE_HZ as f32);
    gen.set_params(params);
    gen.reset();
    (0..n).map(|_| gen.tick([0.0, 0.0])[0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_sine_has_length_and_peak() {
        let s = Stimulus::Sine(SineParams::default()).render(48_000);
        assert_eq!(s.len(), 48_000);
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let want = 10f32.powf(-20.0 / 20.0);
        assert!((peak - want).abs() < 1e-3, "peak {peak} want {want}");
    }

    #[test]
    fn sweep_and_pulse_render() {
        let sweep = Stimulus::Sweep(ExponentialSweepParams::default()).render(1000);
        let pulse = Stimulus::Pulse(PulseTrainParams::default()).render(1000);
        assert_eq!(sweep.len(), 1000);
        assert_eq!(pulse.len(), 1000);
        assert!(sweep.iter().any(|v| *v != 0.0));
        assert!(pulse.iter().any(|v| *v != 0.0));
    }
}
