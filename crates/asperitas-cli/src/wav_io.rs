//! WAV file read/write helpers for stereo 16-bit PCM audio.

use asperitas_dsp::processor::Frame;
use hound::{WavReader, WavSpec, WavWriter};

/// Read a WAV file and return its spec and interleaved stereo frames as f32 in [-1, 1].
///
/// Returns an error if the file is not 16-bit PCM stereo.
pub fn read_wav(path: &str) -> Result<(WavSpec, Vec<Frame>), String> {
    let mut reader = WavReader::open(path).map_err(|e| format!("cannot open '{}': {e}", path))?;

    let spec = reader.spec();
    validate_spec(&spec)?;

    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("error reading samples from '{}': {e}", path))?;

    let frames: Vec<Frame> = samples
        .chunks_exact(2)
        .map(|chunk| {
            [
                chunk[0] as f32 / i16::MAX as f32,
                chunk[1] as f32 / i16::MAX as f32,
            ]
        })
        .collect();

    Ok((spec, frames))
}

/// Write stereo frames to a WAV file with the given spec.
///
/// Samples are clamped to [-1.0, 1.0], scaled to i16, and written as interleaved stereo.
pub fn write_wav(path: &str, spec: WavSpec, frames: &[Frame]) -> Result<(), String> {
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
    if spec.channels != 2 {
        return Err(format!(
            "unsupported channel count: expected stereo (2), got {}",
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
        let tmp = NamedTempFile::new().unwrap();
        let mut writer = WavWriter::create(tmp.path(), spec).unwrap();
        for _ in 0..4 {
            writer.write_sample(0i16).unwrap();
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

    #[test]
    fn validate_spec_rejects_mono() {
        let tmp = make_wav_path(WavSpec {
            channels: 1,
            bits_per_sample: 16,
            sample_rate: 48000,
            sample_format: SampleFormat::Int,
        });
        let err = read_wav(tmp.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("stereo"),
            "expected error to mention stereo, got: {err}"
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
