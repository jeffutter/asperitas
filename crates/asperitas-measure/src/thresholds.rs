//! Every numeric limit the measurement analysis judges against.
//!
//! Nothing outside this module may spell a limit as a literal. Each constant cites where its value
//! came from, so a change to one is a change to a documented decision.

use crate::capture::FULL_SCALE;

/// Samples per fitting block when the discontinuity model uses one fixed gain for the whole
/// capture. The gain is constant, so this only bounds how much of the model is evaluated at once.
///
/// Origin: working-buffer size; any value gives the same result.
pub const FIXED_GAIN_BLOCK_SAMPLES: usize = 4096;

/// Loop gain of the Pod self-loopback at the rig's -20 dBFS sine, in dB (capture minus stimulus).
///
/// Origin: the rig baseline, the 300 s run (`captures/TASK-038.05/2026-10-07-rig-sine-300s.console.zst`).
pub const LOOP_GAIN_DB: f64 = -0.41;

/// Noise plus distortion of that capture, in LSB RMS of the 16-bit capture.
///
/// Origin: the rig baseline, same run.
pub const NOISE_DISTORTION_LSB_RMS: f64 = 0.35;

/// DC offset of that capture, in 16-bit LSB.
///
/// Origin: the rig baseline, same run.
pub const DC_LSB: f64 = -0.49;

/// Alignment window, samples (1 s at 48 kHz). Long enough for a sharp correlation peak against
/// the rig's noise floor, short enough that a slip is localised to a second.
///
/// Origin: chosen for the alignment stage; confirmed against the 2026-10-07 and 2026-10-13 fixtures.
pub const ALIGN_WINDOW_SAMPLES: usize = 48_000;

/// Largest loopback delay searched, samples. Delay is determined modulo the period for a periodic
/// stimulus, so this is also the span over which a sine delay is unambiguous only if below one period.
///
/// Origin: set from measured fixture delays (see tests in `align.rs`).
pub const ALIGN_MAX_DELAY_SAMPLES: usize = 1_000;

/// Largest allowed change in delay between windows or repeat captures, samples.
///
/// Origin: set from measured fixture spread (see tests in `align.rs`).
pub const ALIGN_STABLE_TOL_SAMPLES: f64 = 0.25;

/// Half-width of the delay search around the first window's delay once anchored, samples. Below
/// half the shortest stimulus period (48 samples for the rig's 1 kHz sine) so tracking cannot jump
/// to a neighbouring period, and above any plausible one-window slip.
///
/// Origin: calibrated against the fixtures.
pub const ALIGN_TRACK_SAMPLES: usize = 8;

/// A correlation peak counts as tied with the maximum above this fraction of it. Used only to pick
/// the smallest delay among the equal peaks a periodic stimulus produces, so the anchor is the same
/// across repeat captures.
///
/// Origin: fixture sine peaks agree to far better than this.
pub const ALIGN_TIE_FRACTION: f32 = 0.98;

/// Largest allowed deviation of measured loop gain from [`LOOP_GAIN_DB`], dB.
///
/// Origin: set from the 2026-10-07 and 2026-10-15 fixture deviations (see
/// `metrics.rs` tests) with margin: both fixtures measured within 0.005 dB.
pub const LEVEL_DEVIATION_TOL_DB: f64 = 0.05;

/// A sample counts as silent when its magnitude is at most this, 16-bit LSB.
///
/// Origin: the rig noise floor is 0.35 LSB RMS and DC is -0.49 LSB
/// ([`NOISE_DISTORTION_LSB_RMS`], [`DC_LSB`]), so 2 LSB is several sigma of idle noise.
pub const SILENCE_LSB: i16 = 2;

/// A silent run is reported once it lasts this many samples (1 ms at 48 kHz). The 1 kHz sine
/// crosses zero for only a few samples at a time, so this cannot trigger on the stimulus itself.
///
/// Origin: calibrated against the fixtures.
pub const SILENCE_MIN_RUN_SAMPLES: usize = 48;

/// A discontinuity candidate is a sample where the aligned residual (capture minus gain-scaled,
/// delayed stimulus) jumps by more than this from the previous sample, 16-bit LSB.
///
/// Origin: set from the largest residual jump in the fixtures (see `metrics.rs` tests).
pub const DISCONTINUITY_JUMP_LSB: f64 = 8.0;

/// Candidates closer together than this many samples are one event (a dropout produces a jump at
/// each edge, a step produces one).
///
/// Origin: calibrated against the fixtures.
pub const DISCONTINUITY_MERGE_SAMPLES: usize = 8;

/// Most discontinuity positions stored in the report; the count stays exact.
///
/// Origin: keeps the JSON bounded for a badly broken capture.
pub const DISCONTINUITY_MAX_POSITIONS: usize = 64;

/// Spectrum frame length, samples (a 2.9 Hz bin at 48 kHz, fine enough to separate mains-related
/// hum from the 1 kHz fundamental).
///
/// Origin: calibrated against the fixtures.
pub const SPUR_FFT_SAMPLES: usize = 16_384;

/// Bins either side of the fundamental treated as the fundamental itself (a Hann main lobe is
/// +-2 bins; one more for the peak's off-bin position).
///
/// Origin: calibrated against the fixtures.
pub const SPUR_EXCLUDE_BINS: usize = 3;

/// Number of largest non-fundamental spectral peaks reported.
///
/// Origin: calibrated against the fixtures.
pub const SPUR_REPORT_COUNT: usize = 8;

/// Highest allowed spur level relative to the fundamental, dB (negative).
///
/// Origin: set from the fixtures' measured worst spur (see `metrics.rs` tests).
pub const SPUR_MAX_DBC: f64 = -80.0;

/// Block length for the parameter-step envelope, samples (1 ms at 48 kHz, a whole number of
/// periods of the rig's 1 kHz sine so a steady tone has constant block RMS).
///
/// Origin: calibrated against the fixtures.
pub const PARAM_STEP_BLOCK_SAMPLES: usize = 48;

/// Largest allowed change in block RMS between blocks one or two apart, dB.
///
/// Origin: set from `param_step.rs` tests: a 10.4 dB level change (`gain_db` 10.4) through the
/// smoothed `Gain` (10 ms time constant) peaks at 2.8 dB over two blocks, the same change snapped
/// (unsmoothed) reads 10.4 dB. 5 dB sits between with 1.5x margin either side.
pub const PARAM_STEP_MAX_DB: f64 = 5.0;

/// Blocks quieter than this RMS (linear, full scale 1.0) are ignored, so a dB ratio against idle
/// noise cannot read as a step. About 2 LSB of 16-bit, as [`SILENCE_LSB`].
///
/// Origin: calibrated against the fixtures.
pub const PARAM_STEP_FLOOR_RMS: f32 = (2.0 / FULL_SCALE) as f32;

/// Step flags closer together than this many blocks are one event (a step falling inside a block
/// flags both its boundaries).
///
/// Origin: calibrated against the fixtures.
pub const PARAM_STEP_MERGE_BLOCKS: usize = 3;

/// Most step positions stored in the report; the count stays exact.
///
/// Origin: keeps the JSON bounded, as [`DISCONTINUITY_MAX_POSITIONS`].
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
/// The same budget applies at every `CTL FX` setting: the reference is processed
/// at the commanded setting, so the fitted loop gain stays comparable to [`LOOP_GAIN_DB`] and is
/// judged by [`LEVEL_DEVIATION_TOL_DB`]; a capture processed at another gain misses it by the dB
/// difference, and a low-pass mismatch shows as residual or an alignment slip. Synthetic captures
/// (the fixture with the processor applied host-side) measure residual -30.9 dB for gain at -12,
/// +6 and -38 dB, -32.1 and -33.1 dB for low-pass (the filter removes the attack content that
/// dominates the residual), loop-gain deviation -0.009 dB throughout. Synthetic captures scale or
/// filter the loopback noise with the signal, which a real DAC/ADC would not (its noise floor is
/// fixed), so heavy attenuation on the device may need a looser limit: revisit once measured on the device.
///
/// Origin: set from the `mand_soft` replay fixture (`compare.rs` tests): residual
/// -30.9 dB. -25 dB leaves about 6 dB of margin, while a one-sample misalignment (about -17.5 dB), a
/// wrong excerpt and a 2000-sample dropout all land above it.
pub const COMPARE_RESIDUAL_RMS_DB: f64 = -25.0;

/// Largest allowed residual peak of an excerpt replay, in dB relative to the reference peak. Same
/// reason bit-exactness is not claimed as [`COMPARE_RESIDUAL_RMS_DB`]; the peak is looser because a
/// pluck's attack transient excites the converter filters hardest.
///
/// Origin: set from the `mand_soft` replay fixture: residual peak
/// -21.5 dB (803 LSB at sample 1502, the attack), so -15 dB leaves 6.5 dB of margin.
pub const COMPARE_RESIDUAL_PEAK_DB: f64 = -15.0;

/// Number of equal parameter-value buckets a knob sweep is divided into for the response curve.
///
/// Origin: The rig sweeps at 20 SET/s, so on a 10 s sweep each bucket is 0.5 s and
/// 10 `SET`s; 20 buckets resolve a plateau a tenth of the travel wide while leaving each
/// bucket enough samples to average the response of music.
pub const SWEEP_BUCKETS: usize = 20;

/// Fraction of each bucket's time span discarded at both ends before measuring it. Covers the
/// smoothing settle after the bucket's first `SET` and the unrecorded offset between the capture's
/// start and the first `SET` (the session sidecar's start is an estimate): the central half of a 0.5 s
/// bucket stays valid for an offset error of up to 125 ms.
///
/// Origin: calibrated against the fixtures.
pub const SWEEP_BUCKET_TRIM: f64 = 0.25;

/// Fewest samples a bucket's trimmed window may hold; a shorter sweep is a failed verdict, not a
/// measurement. 10 ms at 48 kHz.
///
/// Origin: calibrated against the fixtures.
pub const SWEEP_MIN_WINDOW_SAMPLES: usize = 480;

/// Smallest total end-to-end response change, dB, for a sweep to count as having a direction. Less
/// than this the knob moved the sound by no more than level wander in the source.
///
/// Origin: set from the synthetic fixtures in `sweep.rs`, whose healthy curves span
/// 24 dB.
pub const SWEEP_MIN_SPAN_DB: f64 = 6.0;

/// Largest allowed fall of the response against the sweep direction, dB, measured from the running
/// best so far (so a slow drift backwards counts, not just one step). Bucket levels of music wander
/// by the dynamics of the passage; this is that allowance.
///
/// Origin: set from the synthetic fixtures: ripple of 0.4 dB RMS on a healthy
/// curve stays under 1.0 dB, a 3 dB reversal sits above it with 3x margin. Revisit with the first
/// real capture on the board.
pub const SWEEP_MONOTONIC_TOL_DB: f64 = 1.0;

/// A bucket-to-bucket response change smaller than this, dB, is "no change".
///
/// Origin: set from the synthetic fixtures: a healthy 24 dB sweep over
/// [`SWEEP_BUCKETS`] moves about 1.3 dB per bucket; 0.5 dB sits below that and above ripple.
pub const SWEEP_FLAT_EPS_DB: f64 = 0.5;

/// Consecutive "no change" bucket steps that make a flat region (a dead zone or plateau). Three
/// steps span four buckets, a fifth of the travel.
///
/// Origin: calibrated against the fixtures.
pub const SWEEP_FLAT_MIN_STEPS: usize = 3;

/// Shortest capture a soak verdict is given for, seconds. The ticket asks for "a 5+ minute
/// capture"; a shorter run is a failed verdict, not a shorter soak.
///
/// Origin: the soak acceptance criterion.
pub const SOAK_MIN_SECONDS: f64 = 300.0;

/// Time added after the end of a parameter sweep, and before its start, that still counts as
/// "parameter motion" when attributing candidates, seconds. Covers the 10 ms smoothing tail
/// (several time constants) and the error in the caller's estimate of when the first `SET` landed
/// (the capture does not record it), as [`SWEEP_BUCKET_TRIM`] does for the sweep analysis.
///
/// Origin: attribution only, never changes a verdict.
pub const SOAK_MOTION_MARGIN_S: f64 = 1.0;

/// Block over which the soak's discontinuity detector refits the loop gain, samples: 10 ms, ten
/// periods of the rig's 1 kHz sine. A sweep changes the gain by a few thousandths of a percent
/// per block, so the residual of a healthy swept capture stays far under
/// [`DISCONTINUITY_JUMP_LSB`], while a dropout or a snapped step still jumps inside the block.
///
/// Origin: set from the synthetic fixtures in `soak.rs`.
pub const SOAK_GAIN_FIT_BLOCK_SAMPLES: usize = 480;
