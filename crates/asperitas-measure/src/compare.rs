//! Compare a replayed-excerpt capture with what `asperitas-cli process` produces for the same audio.
//!
//! The rig replays an installed QSPI excerpt through the DAC; the loopback returns it through the
//! ADC. The reference is the same excerpt PCM run through the processor the capture was taken
//! with ([`FxSetting`], the `CTL FX` selection: bypass, `Gain`, or `OnePoleLowPass`), using the
//! code and the int16-to-float convention `asperitas-cli process` uses. Bypass (the default, and
//! what the device does unless told otherwise) is identity. The device capture at non-identity
//! settings is TASK-035.07.03.
//!
//! Because the reference already contains the commanded processing, the fitted loop gain stays
//! comparable to [`thresholds::LOOP_GAIN_DB`] at every setting: a capture processed with a
//! different gain than the reference shows up as loop-gain deviation (and, for a low-pass, as
//! residual), never as a silent re-normalization.
//!
//! The capture is aligned to the reference, then a least-squares gain and DC offset are fitted and
//! removed (the loop gain and converter offset), and the remaining residual is reported in LSB and
//! in dB relative to the reference. Why that cannot be zero is written at
//! [`thresholds::COMPARE_RESIDUAL_RMS_DB`].

use std::fmt;

use asperitas_dsp::replay_fx::{FxKind, ReplayFx};
use asperitas_logging::excerpt::{parse_wav, WavError};
use asperitas_logging::frame::crc16_ccitt;
use serde::Serialize;

use crate::align::{align, assert_stable, AlignError};
use crate::capture::Capture;
use crate::thresholds;

/// Full scale of the 16-bit capture, LSB.
const FULL_SCALE: f64 = 32768.0;

/// The processor selection of the replay, as `CTL FX f=<code> v=<value>` carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FxSetting {
    /// Wire code: 0 bypass, 1 gain, 2 low-pass ([`FxKind::from_code`]).
    pub code: u8,
    /// Wire value, `0..=0xFFFF` spanning the processor's normalised knob `0.0..=1.0`.
    pub value: u16,
}

impl FxSetting {
    pub const BYPASS: FxSetting = FxSetting { code: 0, value: 0 };

    /// Parse the `--fx` name (`bypass`, `gain`, `lowpass`) and `--fx-value` wire value.
    pub fn from_name(name: &str, value: u16) -> Option<FxSetting> {
        let code = match name {
            "bypass" => 0,
            "gain" => 1,
            "lowpass" => 2,
            _ => return None,
        };
        Some(FxSetting { code, value })
    }

    fn kind(self) -> Option<FxKind> {
        FxKind::from_code(self.code)
    }

    /// The knob the firmware derives from the wire value (`value / u16::MAX`).
    pub fn knob(self) -> f32 {
        f32::from(self.value) / f32::from(u16::MAX)
    }
}

/// The reference side: the excerpt as `asperitas-cli process` renders its left lane at the same
/// processor and parameters the firmware's replay used.
#[derive(Clone, Debug)]
pub struct Reference {
    /// Left-lane samples, float in [-1, 1].
    pub samples: Vec<f32>,
    /// CRC-16/CCITT-FALSE of the excerpt PCM, as the install (`EXCOK`) and replay (`EXCPLAY`) record.
    pub pcm_crc16: u16,
}

impl Reference {
    /// Build the reference from the bytes of a mono 48 kHz 16-bit WAV (an `audio/instruments` clip).
    pub fn from_excerpt_wav(raw: &[u8]) -> Result<Reference, CompareError> {
        Self::from_excerpt_wav_fx(raw, FxSetting::BYPASS)
    }

    /// As [`Reference::from_excerpt_wav`], processed by `fx`.
    pub fn from_excerpt_wav_fx(raw: &[u8], fx: FxSetting) -> Result<Reference, CompareError> {
        let pcm = parse_wav(raw).map_err(CompareError::Wav)?;
        let kind = fx.kind().ok_or(CompareError::UnknownFx(fx.code))?;
        let mut replay = ReplayFx::new();
        replay.select(kind, fx.knob());
        // `asperitas-cli` widens int16 by dividing by i16::MAX and feeds the same value to both lanes.
        let mut frames: Vec<[f32; 2]> = pcm
            .chunks_exact(2)
            .map(|b| {
                let v = f32::from(i16::from_le_bytes([b[0], b[1]])) / f32::from(i16::MAX);
                [v, v]
            })
            .collect();
        replay.process(&mut frames);
        let samples = frames.iter().map(|f| f[0]).collect();
        Ok(Reference {
            samples,
            pcm_crc16: crc16_ccitt(pcm),
        })
    }
}

#[derive(Debug)]
pub enum CompareError {
    Wav(WavError),
    /// The `CTL FX` code names no effect.
    UnknownFx(u8),
    /// The reference is shorter than one alignment window.
    ReferenceTooShort {
        have: usize,
        need: usize,
    },
    /// The capture does not extend past the loopback delay by the reference length.
    CaptureTooShort {
        have: usize,
        need: usize,
    },
    Align(AlignError),
    /// The residual is outside the documented tolerance.
    OutOfTolerance(String),
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompareError::Wav(e) => write!(f, "excerpt WAV rejected: {e:?}"),
            CompareError::UnknownFx(c) => write!(f, "unknown effect code {c}"),
            CompareError::ReferenceTooShort { have, need } => {
                write!(f, "reference has {have} samples, need at least {need}")
            }
            CompareError::CaptureTooShort { have, need } => {
                write!(f, "capture has {have} samples, need {need}")
            }
            CompareError::Align(e) => write!(f, "alignment failed: {e}"),
            CompareError::OutOfTolerance(m) => write!(f, "outside tolerance: {m}"),
        }
    }
}

impl std::error::Error for CompareError {}

/// Residual of an aligned, gain- and DC-normalized capture against the reference.
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    /// Compared samples (the reference length).
    pub samples: usize,
    /// Loopback delay used, whole samples (the median window delay, rounded).
    pub delay_samples: usize,
    /// Fitted loop gain, dB, and its deviation from [`thresholds::LOOP_GAIN_DB`].
    pub loop_gain_db: f64,
    pub gain_deviation_db: f64,
    /// Fitted DC offset of the capture, LSB.
    pub dc_lsb: f64,
    pub reference_rms_lsb: f64,
    pub reference_peak_lsb: f64,
    pub residual_rms_lsb: f64,
    pub residual_peak_lsb: f64,
    /// Reference-relative residual, dB (negative: residual below the reference).
    pub residual_rms_db: f64,
    pub residual_peak_db: f64,
    /// Sample (reference index) of the residual peak.
    pub residual_peak_at: usize,
}

impl Comparison {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Comparison serialises")
    }

    /// Ok when the residual and the fitted loop gain are within the documented tolerances.
    pub fn check(&self) -> Result<(), CompareError> {
        let mut bad = Vec::new();
        if self.residual_rms_db > thresholds::COMPARE_RESIDUAL_RMS_DB {
            bad.push(format!(
                "residual RMS {:.1} dB > {:.1} dB",
                self.residual_rms_db,
                thresholds::COMPARE_RESIDUAL_RMS_DB
            ));
        }
        if self.residual_peak_db > thresholds::COMPARE_RESIDUAL_PEAK_DB {
            bad.push(format!(
                "residual peak {:.1} dB > {:.1} dB",
                self.residual_peak_db,
                thresholds::COMPARE_RESIDUAL_PEAK_DB
            ));
        }
        if self.gain_deviation_db.abs() > thresholds::LEVEL_DEVIATION_TOL_DB {
            bad.push(format!(
                "loop gain {:.3} dB deviates {:.3} dB from baseline",
                self.loop_gain_db, self.gain_deviation_db
            ));
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(CompareError::OutOfTolerance(bad.join("; ")))
        }
    }
}

/// Compare a replay capture (excerpt played once from the start of the capture, then silence)
/// with `reference`.
pub fn compare(capture: &Capture, reference: &Reference) -> Result<Comparison, CompareError> {
    let r = &reference.samples;
    let w = thresholds::ALIGN_WINDOW_SAMPLES;
    let n_win = r.len() / w;
    if n_win == 0 {
        return Err(CompareError::ReferenceTooShort {
            have: r.len(),
            need: w,
        });
    }
    // Align on whole windows the excerpt fully covers; the silence after it has nothing to correlate.
    let head = Capture {
        samples: capture.samples[..(n_win * w).min(capture.samples.len())].to_vec(),
        sample_rate_hz: capture.sample_rate_hz,
        blocks: capture.blocks,
    };
    let alignment = align(&head, r).map_err(CompareError::Align)?;
    assert_stable(&alignment).map_err(CompareError::Align)?;
    let delay = alignment.median().round().max(0.0) as usize;

    let need = delay + r.len();
    if capture.samples.len() < need {
        return Err(CompareError::CaptureTooShort {
            have: capture.samples.len(),
            need,
        });
    }
    let n = r.len() as f64;
    let m: Vec<f64> = r.iter().map(|&v| f64::from(v) * FULL_SCALE).collect();
    let x: Vec<f64> = capture.samples[delay..need]
        .iter()
        .map(|&s| f64::from(s))
        .collect();

    // Least squares x ~= g*m + dc.
    let (mean_x, mean_m) = (x.iter().sum::<f64>() / n, m.iter().sum::<f64>() / n);
    let (mut sxm, mut smm) = (0.0, 0.0);
    for (xv, mv) in x.iter().zip(&m) {
        sxm += (xv - mean_x) * (mv - mean_m);
        smm += (mv - mean_m) * (mv - mean_m);
    }
    let g = sxm / smm.max(1e-12);
    let dc = mean_x - g * mean_m;

    let (mut sum_sq, mut peak, mut peak_at) = (0.0, 0.0f64, 0usize);
    for (i, (xv, mv)) in x.iter().zip(&m).enumerate() {
        let res = xv - g * mv - dc;
        sum_sq += res * res;
        if res.abs() > peak {
            peak = res.abs();
            peak_at = i;
        }
    }
    let residual_rms = (sum_sq / n).sqrt();
    // Reference level as it appears at the capture: scaled by the fitted loop gain.
    let ref_rms = g.abs() * (m.iter().map(|v| v * v).sum::<f64>() / n).sqrt();
    let ref_peak = g.abs() * m.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    let db = |num: f64, den: f64| 20.0 * (num.max(1e-12) / den.max(1e-12)).log10();
    let loop_gain_db = 20.0 * g.abs().max(1e-12).log10();
    Ok(Comparison {
        samples: r.len(),
        delay_samples: delay,
        loop_gain_db,
        gain_deviation_db: loop_gain_db - thresholds::LOOP_GAIN_DB,
        dc_lsb: dc,
        reference_rms_lsb: ref_rms,
        reference_peak_lsb: ref_peak,
        residual_rms_lsb: residual_rms,
        residual_peak_lsb: peak,
        residual_rms_db: db(residual_rms, ref_rms),
        residual_peak_db: db(peak, ref_peak),
        residual_peak_at: peak_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(p: &str) -> String {
        format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))
    }

    fn replay() -> Capture {
        Capture::from_console_zst(root(
            "audio/captures/2026-10-15-rig-excerpt-replay-mand-soft.console.zst",
        ))
        .unwrap()
    }

    fn clip(name: &str) -> Reference {
        Reference::from_excerpt_wav(
            &std::fs::read(root(&format!("audio/instruments/{name}.wav"))).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn reference_matches_the_installed_excerpt_and_the_cli_golden() {
        let r = clip("mandolin_single_note_soft");
        // EXCOK/EXCPLAY in the committed install/replay captures: crc16=15fd bytes=288000.
        assert_eq!(r.pcm_crc16, 0x15fd);
        assert_eq!(r.samples.len(), 144_000);
        // The committed `asperitas-cli process gain` golden (stereo int16, canonical 44-byte header)
        // is the CLI's own output for this clip at default (0 dB) parameters.
        let golden = std::fs::read(root(
            "audio/goldens/gain_default_mandolin_single_note_soft.wav",
        ))
        .unwrap();
        let pcm = &golden[44..];
        assert_eq!(pcm.len(), r.samples.len() * 4);
        for (i, (&v, frame)) in r.samples.iter().zip(pcm.chunks_exact(4)).enumerate() {
            let left = i16::from_le_bytes([frame[0], frame[1]]);
            assert_eq!(
                (v.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16,
                left,
                "sample {i}"
            );
        }
    }

    #[test]
    fn replay_capture_is_within_tolerance() {
        let c = compare(&replay(), &clip("mandolin_single_note_soft")).unwrap();
        eprintln!("{}", c.to_json());
        c.check().unwrap();
    }

    #[test]
    fn wrong_excerpt_fails() {
        let wrong = clip("mandolin_single_note_hard");
        let verdict = compare(&replay(), &wrong).and_then(|c| c.check());
        eprintln!("{verdict:?}");
        assert!(verdict.is_err());
    }

    #[test]
    fn time_shifted_reference_fails() {
        // Splice: the second half of the reference arrives one sample late. A uniform shift is
        // absorbed by alignment (it is just a different latency); a mid-excerpt shift is a slip.
        let mut r = clip("mandolin_single_note_soft");
        let half = r.samples.len() / 2;
        r.samples.insert(half, 0.0);
        r.samples.pop();
        let verdict = compare(&replay(), &r).and_then(|c| c.check());
        eprintln!("{verdict:?}");
        assert!(verdict.is_err());
    }

    #[test]
    fn dropout_in_capture_fails() {
        let mut c = replay();
        for s in &mut c.samples[20_000..22_000] {
            *s = 0;
        }
        let verdict = compare(&c, &clip("mandolin_single_note_soft")).and_then(|c| c.check());
        eprintln!("{verdict:?}");
        assert!(verdict.is_err());
    }

    const MAND_SOFT: &str = "mandolin_single_note_soft";

    fn clip_fx(fx: FxSetting) -> Reference {
        Reference::from_excerpt_wav_fx(
            &std::fs::read(root(&format!("audio/instruments/{MAND_SOFT}.wav"))).unwrap(),
            fx,
        )
        .unwrap()
    }

    /// The replay capture as a device running `fx` would have returned it, synthesized host-side:
    /// the real loopback capture (converters, noise, loop gain) with the processor applied to it.
    /// The converters are close to linear, so processing before or after them agrees to well
    /// within the residual budget.
    fn synth(fx: FxSetting) -> Capture {
        let mut c = replay();
        let mut replay_fx = ReplayFx::new();
        replay_fx.select(fx.kind().unwrap(), fx.knob());
        let mut frames: Vec<[f32; 2]> = c
            .samples
            .iter()
            .map(|&s| {
                let v = f32::from(s) / 32768.0;
                [v, v]
            })
            .collect();
        replay_fx.process(&mut frames);
        for (s, f) in c.samples.iter_mut().zip(&frames) {
            *s = (f[0] * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
        c
    }

    fn fx(name: &str, value: u16) -> FxSetting {
        FxSetting::from_name(name, value).unwrap()
    }

    // Gain knob v/65535: 0.6667 = -12 dB, 0.8333 = 0 dB, 0.9167 = +6 dB. Low-pass log taper.
    const SETTINGS: [(&str, u16); 5] = [
        ("gain", 43_690),
        ("gain", 58_982),
        ("gain", 20_000),
        ("lowpass", 52_000),
        ("lowpass", 40_000),
    ];

    #[test]
    fn processed_captures_match_their_own_settings() {
        for (name, v) in SETTINGS {
            let s = fx(name, v);
            let c = compare(&synth(s), &clip_fx(s)).unwrap();
            eprintln!("{name} {v}: {}", c.to_json());
            c.check().unwrap_or_else(|e| panic!("{name} {v}: {e}"));
        }
    }

    #[test]
    fn bypass_setting_is_the_identity_reference() {
        assert_eq!(clip_fx(FxSetting::BYPASS).samples, clip(MAND_SOFT).samples);
    }

    #[test]
    fn capture_processed_with_a_different_setting_fails() {
        for ((name, v), (rname, rv)) in [
            (("gain", 43_690), ("gain", 58_982)),
            (("gain", 43_690), ("gain", 40_000)),
            (("lowpass", 52_000), ("lowpass", 40_000)),
            (("lowpass", 52_000), ("bypass", 0)),
            (("gain", 43_690), ("bypass", 0)),
        ] {
            let verdict =
                compare(&synth(fx(name, v)), &clip_fx(fx(rname, rv))).and_then(|c| c.check());
            eprintln!("{name} {v} vs {rname} {rv}: {verdict:?}");
            assert!(verdict.is_err(), "{name} {v} vs {rname} {rv} passed");
        }
    }

    #[test]
    fn unknown_fx_code_is_a_typed_error() {
        let raw = std::fs::read(root(&format!("audio/instruments/{MAND_SOFT}.wav"))).unwrap();
        let bad = FxSetting { code: 9, value: 0 };
        assert!(matches!(
            Reference::from_excerpt_wav_fx(&raw, bad),
            Err(CompareError::UnknownFx(9))
        ));
    }

    #[test]
    fn short_inputs_are_typed_errors() {
        let c = replay();
        let tiny = Reference {
            samples: vec![0.0; 100],
            pcm_crc16: 0,
        };
        assert!(matches!(
            compare(&c, &tiny),
            Err(CompareError::ReferenceTooShort { .. })
        ));
        let short = Capture {
            samples: c.samples[..144_000].to_vec(),
            ..c
        };
        assert!(matches!(
            compare(&short, &clip("mandolin_single_note_soft")),
            Err(CompareError::CaptureTooShort { .. })
        ));
    }
}
