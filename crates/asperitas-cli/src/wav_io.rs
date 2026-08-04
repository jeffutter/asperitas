//! WAV file read/write helpers for 16-bit PCM audio.
//!
//! Reads mono or stereo; always writes stereo, because the DSP pipeline works in
//! stereo [`Frame`]s throughout.

use asperitas_dsp::processor::Frame;
use hound::{WavReader, WavSpec, WavWriter};

/// Read a WAV file and return its spec and interleaved stereo frames as f32 in [-1, 1].
///
/// Mono input is widened to stereo by copying the single channel into both slots, so
/// callers never have to branch on channel count. The returned spec still reports the
/// file's original channel count.
///
/// Returns an error if the file is not 16-bit PCM mono or stereo.
pub fn read_wav(path: &str) -> Result<(WavSpec, Vec<Frame>), String> {
    let mut reader = WavReader::open(path).map_err(|e| format!("cannot open '{}': {e}", path))?;

    let spec = reader.spec();
    validate_spec(&spec)?;

    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("error reading samples from '{}': {e}", path))?;

    let frames: Vec<Frame> = if spec.channels == 1 {
        samples
            .iter()
            .map(|&s| {
                let v = s as f32 / i16::MAX as f32;
                [v, v]
            })
            .collect()
    } else {
        samples
            .chunks_exact(2)
            .map(|chunk| {
                [
                    chunk[0] as f32 / i16::MAX as f32,
                    chunk[1] as f32 / i16::MAX as f32,
                ]
            })
            .collect()
    };

    Ok((spec, frames))
}

/// Write stereo frames to a WAV file at `sample_rate`.
///
/// Samples are clamped to [-1.0, 1.0], scaled to i16, and written as interleaved stereo.
///
/// The channel count, bit depth, and sample format are chosen here rather than supplied
/// by the caller, because they are fixed by what this function actually writes. Taking a
/// full [`WavSpec`] let a caller pass one that disagreed with the interleaved-stereo
/// payload — reusing a mono input's spec produced a file whose header claimed one channel
/// while holding two.
pub fn write_wav(path: &str, sample_rate: u32, frames: &[Frame]) -> Result<(), String> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(path, spec).map_err(|e| format!("cannot create '{}': {e}", path))?;

    for frame in frames {
        let l = (frame[0].clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        let r = (frame[1].clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(l)
            .map_err(|e| format!("error writing sample: {e}"))?;
        writer
            .write_sample(r)
            .map_err(|e| format!("error writing sample: {e}"))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("error finalizing '{}': {e}", path))?;
    Ok(())
}

fn validate_spec(spec: &WavSpec) -> Result<(), String> {
    if spec.sample_format != hound::SampleFormat::Int {
        return Err(format!(
            "unsupported sample format: expected Int, got {:?}",
            spec.sample_format
        ));
    }
    if spec.bits_per_sample != 16 {
        return Err(format!(
            "unsupported bit depth: expected 16, got {}",
            spec.bits_per_sample
        ));
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
    fn validate_spec_rejects_float_format() {
        let tmp = make_wav_path(WavSpec {
            channels: 2,
            bits_per_sample: 32,
            sample_rate: 48000,
            sample_format: SampleFormat::Float,
        });
        let err = read_wav(tmp.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("Int"),
            "expected error to mention Int format, got: {err}"
        );
    }
}
