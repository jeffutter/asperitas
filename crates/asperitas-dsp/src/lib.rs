#![no_std]

/// Process a single audio sample.
///
/// Currently a no-op passthrough; real DSP will replace this.
pub fn process_sample(sample: f32) -> f32 {
    sample
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_sample_passthrough() {
        assert_eq!(process_sample(0.5), 0.5);
        assert_eq!(process_sample(-1.0), -1.0);
        assert_eq!(process_sample(0.0), 0.0);
    }
}
