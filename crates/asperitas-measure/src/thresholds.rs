//! Every numeric limit the measurement analysis judges against.
//!
//! Nothing outside this module may spell a limit as a literal. Each constant cites where its value
//! came from, so a change to one is a change to a documented decision.

/// Loop gain of the Pod self-loopback at the rig's -20 dBFS sine, in dB (capture minus stimulus).
///
/// Origin: TASK-038.05 baseline, 2026-10-07 300 s run (`audio/captures/2026-10-07-rig-sine-300s.console.zst`).
pub const LOOP_GAIN_DB: f64 = -0.41;

/// Noise plus distortion of that capture, in LSB RMS of the 16-bit capture.
///
/// Origin: TASK-038.05 baseline, same run.
pub const NOISE_DISTORTION_LSB_RMS: f64 = 0.35;

/// DC offset of that capture, in 16-bit LSB.
///
/// Origin: TASK-038.05 baseline, same run.
pub const DC_LSB: f64 = -0.49;

/// Alignment window, samples (1 s at 48 kHz). Long enough for a sharp correlation peak against
/// the rig's noise floor, short enough that a slip is localised to a second.
///
/// Origin: chosen for TASK-035.02; confirmed against the 2026-10-07 and 2026-10-13 fixtures.
pub const ALIGN_WINDOW_SAMPLES: usize = 48_000;

/// Largest loopback delay searched, samples. Delay is determined modulo the period for a periodic
/// stimulus, so this is also the span over which a sine delay is unambiguous only if below one period.
///
/// Origin: TASK-035.02, set from measured fixture delays (see tests in `align.rs`).
pub const ALIGN_MAX_DELAY_SAMPLES: usize = 1_000;

/// Largest allowed change in delay between windows or repeat captures, samples.
///
/// Origin: TASK-035.02, set from measured fixture spread (see tests in `align.rs`).
pub const ALIGN_STABLE_TOL_SAMPLES: f64 = 0.25;

/// Half-width of the delay search around the first window's delay once anchored, samples. Below
/// half the shortest stimulus period (48 samples for the rig's 1 kHz sine) so tracking cannot jump
/// to a neighbouring period, and above any plausible one-window slip.
///
/// Origin: TASK-035.02.
pub const ALIGN_TRACK_SAMPLES: usize = 8;

/// A correlation peak counts as tied with the maximum above this fraction of it. Used only to pick
/// the smallest delay among the equal peaks a periodic stimulus produces, so the anchor is the same
/// across repeat captures.
///
/// Origin: TASK-035.02; fixture sine peaks agree to far better than this.
pub const ALIGN_TIE_FRACTION: f32 = 0.98;

/// Largest allowed deviation of measured loop gain from [`LOOP_GAIN_DB`], dB.
///
/// Origin: TASK-035.03, set from the 2026-10-07 and 2026-10-15 fixture deviations (see
/// `metrics.rs` tests) with margin: both fixtures measured within 0.005 dB.
pub const LEVEL_DEVIATION_TOL_DB: f64 = 0.05;

/// A sample counts as silent when its magnitude is at most this, 16-bit LSB.
///
/// Origin: TASK-035.03; the rig noise floor is 0.35 LSB RMS and DC is -0.49 LSB
/// ([`NOISE_DISTORTION_LSB_RMS`], [`DC_LSB`]), so 2 LSB is several sigma of idle noise.
pub const SILENCE_LSB: i16 = 2;

/// A silent run is reported once it lasts this many samples (1 ms at 48 kHz). The 1 kHz sine
/// crosses zero for only a few samples at a time, so this cannot trigger on the stimulus itself.
///
/// Origin: TASK-035.03.
pub const SILENCE_MIN_RUN_SAMPLES: usize = 48;

/// A discontinuity candidate is a sample where the aligned residual (capture minus gain-scaled,
/// delayed stimulus) jumps by more than this from the previous sample, 16-bit LSB.
///
/// Origin: TASK-035.03, set from the largest residual jump in the fixtures (see `metrics.rs` tests).
pub const DISCONTINUITY_JUMP_LSB: f64 = 8.0;

/// Candidates closer together than this many samples are one event (a dropout produces a jump at
/// each edge, a step produces one).
///
/// Origin: TASK-035.03.
pub const DISCONTINUITY_MERGE_SAMPLES: usize = 8;

/// Most discontinuity positions stored in the report; the count stays exact.
///
/// Origin: TASK-035.03; keeps the JSON bounded for a badly broken capture.
pub const DISCONTINUITY_MAX_POSITIONS: usize = 64;

/// Spectrum frame length, samples (a 2.9 Hz bin at 48 kHz, fine enough to separate mains-related
/// hum from the 1 kHz fundamental).
///
/// Origin: TASK-035.03.
pub const SPUR_FFT_SAMPLES: usize = 16_384;

/// Bins either side of the fundamental treated as the fundamental itself (a Hann main lobe is
/// +-2 bins; one more for the peak's off-bin position).
///
/// Origin: TASK-035.03.
pub const SPUR_EXCLUDE_BINS: usize = 3;

/// Number of largest non-fundamental spectral peaks reported.
///
/// Origin: TASK-035.03.
pub const SPUR_REPORT_COUNT: usize = 8;

/// Highest allowed spur level relative to the fundamental, dB (negative).
///
/// Origin: TASK-035.03, set from the fixtures' measured worst spur (see `metrics.rs` tests).
pub const SPUR_MAX_DBC: f64 = -80.0;

/// Block length for the parameter-step envelope, samples (1 ms at 48 kHz, a whole number of
/// periods of the rig's 1 kHz sine so a steady tone has constant block RMS).
///
/// Origin: TASK-035.04.
pub const PARAM_STEP_BLOCK_SAMPLES: usize = 48;

/// Largest allowed change in block RMS between blocks one or two apart, dB.
///
/// Origin: TASK-035.04, set from `param_step.rs` tests: a 10.4 dB level change (`gain_db` 24) through the
/// smoothed `Gain` (10 ms time constant) peaks at 2.8 dB over two blocks, the same change snapped
/// (unsmoothed) reads 10.4 dB. 5 dB sits between with 1.5x margin either side.
pub const PARAM_STEP_MAX_DB: f64 = 5.0;

/// Blocks quieter than this RMS (linear, full scale 1.0) are ignored, so a dB ratio against idle
/// noise cannot read as a step. About 2 LSB of 16-bit, as [`SILENCE_LSB`].
///
/// Origin: TASK-035.04.
pub const PARAM_STEP_FLOOR_RMS: f32 = 2.0 / 32768.0;

/// Step flags closer together than this many blocks are one event (a step falling inside a block
/// flags both its boundaries).
///
/// Origin: TASK-035.04.
pub const PARAM_STEP_MERGE_BLOCKS: usize = 3;

/// Most step positions stored in the report; the count stays exact.
///
/// Origin: TASK-035.04; keeps the JSON bounded, as [`DISCONTINUITY_MAX_POSITIONS`].
pub const PARAM_STEP_MAX_POSITIONS: usize = 64;

/// Largest allowed residual RMS of an excerpt replay against the `asperitas-cli` reference, in dB
/// relative to the reference RMS, after alignment, loop-gain normalization and DC removal.
///
/// Bit-exactness is not claimed and cannot be. The device path is excerpt PCM, DAC encoder, analog
/// loopback cable, ADC, so the capture differs from the reference by: two converters, each with its
/// own reconstruction/decimation filter (a small frequency-dependent gain and phase, which makes the
/// residual scale with the signal rather than sit at a fixed floor); an analog noise floor of about
/// 16 bits ([`NOISE_DISTORTION_LSB_RMS`], 0.35 LSB RMS); a loop gain of [`LOOP_GAIN_DB`] that the
/// comparison fits rather than assumes; and a DC offset ([`DC_LSB`]) that it removes. Of those the
/// converter response dominates for a loud musical signal, so the limit is relative to the reference.
///
/// Origin: TASK-035.05, set from the `mand_soft` replay fixture (`compare.rs` tests): residual
/// -30.9 dB. -25 dB leaves about 6 dB of margin, while a one-sample misalignment (about -17.5 dB), a
/// wrong excerpt and a 2000-sample dropout all land above it.
pub const COMPARE_RESIDUAL_RMS_DB: f64 = -25.0;

/// Largest allowed residual peak of an excerpt replay, in dB relative to the reference peak. Same
/// reason bit-exactness is not claimed as [`COMPARE_RESIDUAL_RMS_DB`]; the peak is looser because a
/// pluck's attack transient excites the converter filters hardest.
///
/// Origin: TASK-035.05, set from the `mand_soft` replay fixture: residual peak
/// -21.5 dB (803 LSB at sample 1502, the attack), so -15 dB leaves 6.5 dB of margin.
pub const COMPARE_RESIDUAL_PEAK_DB: f64 = -15.0;
