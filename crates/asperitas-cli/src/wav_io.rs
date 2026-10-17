//! WAV file read/write helpers.
//!
//! Reads mono or stereo, 16-bit PCM or 32-bit float; always writes stereo 32-bit float,
//! because the DSP pipeline works in stereo [`Frame`]s throughout and float output keeps
//! every bit of the processed signal (no clamping, no quantisation).

use asperitas_dsp::processor::Frame;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

/// Read a WAV file and return its spec and interleaved stereo frames as f32.
///
/// 16-bit PCM is scaled by `i16::MAX` into [-1, 1]; 32-bit float is taken as is, so values
/// outside [-1, 1] survive. Mono input is widened to stereo by copying the single channel
/// into both slots, so callers never have to branch on channel count. The returned spec
/// still reports the file's original channel count.
///
/// Returns an error if the file is not 16-bit PCM or 32-bit float, mono or stereo.
pub fn read_wav(path: &str) -> Result<(WavSpec, Vec<Frame>), String> {
    let mut reader = WavReader::open(path).map_err(|e| format!("cannot open '{}': {e}", path))?;

    let spec = reader.spec();
    validate_spec(&spec)?;

    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>(),
        SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>(),
    }
    .map_err(|e| format!("error reading samples from '{}': {e}", path))?;

    let frames: Vec<Frame> = if spec.channels == 1 {
        samples.iter().map(|&v| [v, v]).collect()
    } else {
        samples
            .chunks_exact(2)
            .map(|chunk| [chunk[0], chunk[1]])
            .collect()
    };

    Ok((spec, frames))
}

/// Write stereo frames to a 32-bit float WAV file at `sample_rate`.
///
/// Samples are written unmodified as interleaved stereo.
///
/// The channel count, bit depth, and sample format are chosen here rather than supplied
/// by the caller, because they are fixed by what this function actually writes. Taking a
/// full [`WavSpec`] let a caller pass one that disagreed with the interleaved-stereo
/// payload - reusing a mono input's spec produced a file whose header claimed one channel
/// while holding two.
pub fn write_wav(path: &str, sample_rate: u32, frames: &[Frame]) -> Result<(), String> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer =
        WavWriter::create(path, spec).map_err(|e| format!("cannot create '{}': {e}", path))?;

    for frame in frames {
        for &sample in frame {
            writer
                .write_sample(sample)
                .map_err(|e| format!("error writing sample: {e}"))?;
        }
    }

    writer
        .finalize()
        .map_err(|e| format!("error finalizing '{}': {e}", path))?;
    Ok(())
}

fn validate_spec(spec: &WavSpec) -> Result<(), String> {
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) | (SampleFormat::Float, 32) => {}
        (SampleFormat::Int, bits) => {
            return Err(format!(
                "unsupported bit depth: expected 16 for Int, got {bits}"
            ))
        }
        (SampleFormat::Float, bits) => {
            return Err(format!(
                "unsupported bit depth: expected 32 for Float, got {bits}"
            ))
        }
    }
    if spec.channels != 1 && spec.channels != 2 {
        return Err(format!(
            "unsupported channel count: expected mono (1) or stereo (2), got {}",
            spec.channels
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec};
    use tempfile::NamedTempFile;

    fn make_wav_path(spec: WavSpec) -> NamedTempFile {
        make_wav_with_samples(spec, &[0i16; 6])
    }

    fn make_wav_with_samples(spec: WavSpec, samples: &[i16]) -> NamedTempFile {
        let tmp = NamedTempFile::new().unwrap();
        let mut writer = WavWriter::create(tmp.path(), spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        tmp
    }

    #[test]
    fn validate_spec_rejects_wrong_bit_depth() {
        let tmp = make_wav_path(WavSpec {
            channels: 2,
            bits_per_sample: 8,
            sample_rate: 48000,
            sample_format: SampleFormat::Int,
        });
        let err = read_wav(tmp.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("16"),
            "expected error to mention 16-bit requirement, got: {err}"
        );
    }

    /// The instrument corpus in `audio/instruments/` is mono, so mono input must be
    /// accepted and widened rather than rejected.
    #[test]
    fn read_wav_widens_mono_to_stereo() {
        let samples = [1000i16, -2000, 3000];
        let tmp = make_wav_with_samples(
            WavSpec {
                channels: 1,
                bits_per_sample: 16,
                sample_rate: 48000,
                sample_format: SampleFormat::Int,
            },
            &samples,
        );

        let (spec, frames) = read_wav(tmp.path().to_str().unwrap()).unwrap();

        assert_eq!(
            spec.channels, 1,
            "spec should report the file's real layout"
        );
        assert_eq!(frames.len(), samples.len(), "one frame per mono sample");
        for (frame, &sample) in frames.iter().zip(samples.iter()) {
            let expected = sample as f32 / i16::MAX as f32;
            assert_eq!(frame[0], expected);
            assert_eq!(frame[1], expected, "mono must be copied into both channels");
        }
    }

    #[test]
    fn validate_spec_rejects_more_than_two_channels() {
        let tmp = make_wav_path(WavSpec {
            channels: 3,
            bits_per_sample: 16,
            sample_rate: 48000,
            sample_format: SampleFormat::Int,
        });
        let err = read_wav(tmp.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("channel count"),
            "expected error to mention channel count, got: {err}"
        );
    }

    #[test]
    fn validate_spec_rejects_16_bit_float() {
        let spec = WavSpec {
            channels: 2,
            bits_per_sample: 16,
            sample_rate: 48000,
            sample_format: SampleFormat::Float,
        };
        // hound refuses to write 16-bit float, so check the validator directly.
        let err = validate_spec(&spec).unwrap_err();
        assert!(err.contains("32"), "got: {err}");
    }

    /// Float output must round-trip bit for bit, including values a 16-bit file would clamp.
    #[test]
    fn write_then_read_float_is_exact_and_unclamped() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let frames: Vec<Frame> = vec![
            [0.0, -0.0],
            [1.0e-7, -3.3e-6],
            [2.5, -4.0],
            [0.123_456_79, -0.987_654_3],
        ];
        write_wav(path, 44_100, &frames).unwrap();

        let (spec, back) = read_wav(path).unwrap();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.bits_per_sample, 32);
        assert_eq!(spec.sample_format, SampleFormat::Float);
        assert_eq!(spec.sample_rate, 44_100);
        assert_eq!(back.len(), frames.len());
        for (a, b) in frames.iter().zip(&back) {
            assert_eq!(a[0].to_bits(), b[0].to_bits());
            assert_eq!(a[1].to_bits(), b[1].to_bits());
        }
    }
}
