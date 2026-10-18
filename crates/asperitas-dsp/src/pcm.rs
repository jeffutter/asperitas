//! 16-bit PCM scaling shared by every reader of captured or file audio.

/// Scale an `i16` sample into `[-1.0, 1.0)`.
///
/// Divides by 32768, so the scale is symmetric about zero: `i16::MIN` is exactly `-1.0` and
/// `i16::MAX` is just under `1.0`. Dividing by `i16::MAX` would read `i16::MIN` as `-1.00003`.
#[inline]
pub fn i16_to_f32(v: i16) -> f32 {
    f32::from(v) / 32_768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_is_symmetric() {
        assert_eq!(i16_to_f32(i16::MIN), -1.0);
        assert!(i16_to_f32(i16::MAX) < 1.0);
        assert_eq!(i16_to_f32(0), 0.0);
        assert_eq!(i16_to_f32(-16_384), -0.5);
    }
}
