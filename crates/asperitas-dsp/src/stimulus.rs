//! Deterministic stimulus sources for on-device measurement.
//!
//! These waveforms exist so that a capture taken through the analog loop can be *deconvolved*:
//! the analysis subtracts the played waveform from the recorded one, so the host has to be able
//! to reproduce the stimulus bit for bit. That is why this module lives here instead of in the
//! host tooling — firmware and host link the same source, so agreement is a property of the
//! build rather than something two implementations have to maintain.
//!
//! Three sources, chosen for what measurement needs rather than for variety:
//!
//! - [`Sine`] — a tone, for level, THD, THD+N, SNR and SFDR. Integer phase accumulator, so
//!   frequency is exact and amplitude cannot drift over a long capture.
//! - [`ExponentialSweep`] — an exponential chirp (ESS), for impulse response plus distortion in
//!   one pass. Equal energy per octave keeps the recovered response usable and pushes harmonic
//!   distortion away from the fundamental in time.
//! - [`PulseTrain`] — a band-limited pulse, for broadband response and timing without the
//!   out-of-band energy of a one-sample click.
//!
//! ## Level convention — read this before comparing against a meter
//!
//! `level_dbfs` is **peak** magnitude against sample full scale: 0 dBFS is a sample of ±1.0, so
//! −20 dBFS means a peak of 0.1 and nothing else. The default is −20 dBFS, the level the rig
//! calibration records loop gain at.
//!
//! This is not the only convention in measurement practice, and the difference is exactly the
//! crest factor of the tone: a peak-referenced full-scale sine has an r.m.s. amplitude 3.01 dB
//! lower, so any meter or standard that references 0 dBFS to an *r.m.s.* sine reports the same
//! signal as roughly −3 dB quieter (loudness accounting per ITU-R BS.1770, for instance, calls a
//! full-scale sine −3.01 LKFS). Before normalising a measured return against a known loop gain,
//! confirm which reference that gain was measured with.
//!
//! ## Float discipline
//!
//! Every generator computes in `f64` through `libm`, never through `f32::sin`/`f32::powf`. Two
//! reasons, the same ones that made [`crate`]'s other modules use `libm`: `libm` is a pure-Rust
//! port of MUSL and returns identical bits everywhere, while the platform libm behind `std`'s
//! float methods differs between libc implementations; and the sweep's phase integral reaches
//! ~145,000 radians over its default eight seconds, where an `f32` ULP is ~0.017 rad — twenty
//! times the tolerance any of this analysis will accept. The M7 FPU makes the cost irrelevant at
//! 48 kHz.
//!
//! Note the asymmetry: only the sweep accumulates *angle* in `f64`. The sine's phase register is
//! an integer residue, which is exact rather than merely accurate, and the pulse is indexed by a
//! counter, which needs no accumulator at all.
//!
//! ## Parameters are frozen mid-capture
//!
//! Unlike [`crate::Gain`] and [`crate::OnePoleLowPass`], these sources do not smooth parameter
//! changes, and they deliberately ignore their input frames. A smoothed level ramp applied to a
//! stimulus corrupts the very calibration the capture exists to measure, so parameters take
//! effect immediately and the expectation is that nobody changes them while a capture runs.
//!
//! Because of that, [`Processor::set_params`] on [`ExponentialSweep`] is not audio-path cheap:
//! it scans the whole record once to find the true peak (see there). Call it from the control
//! path before arming playback, not from the audio interrupt.

use core::fmt::Write as _;

use libm::{cos, floor, log, pow, round, sin};

use crate::processor::{Frame, Processor};

/// Raised-cosine fade applied to both ends of the sweep, in samples (10 ms at 48 kHz).
///
/// A const rather than a parameter: device and host compile this file, so a shared constant is
/// agreement by construction, whereas a knob would be a decision nobody made.
const FADE_SAMPLES: usize = 480;

/// Smallest ratio `f1/f0` accepted by [`ExponentialSweep`].
///
/// A chirp whose endpoints coincide divides by `ln(f1/f0) = 0`; this floor keeps that logarithm
/// near `1e-3` instead, which degrades into a nearly stationary tone rather than a NaN.
const MIN_CHIRP_RATIO: f64 = 1.001;

/// Sample rate used when none was set, matching the rest of the crate.
const DEFAULT_SAMPLE_RATE_HZ: u32 = 48_000;

/// Practical floor for `level_dbfs`, well below the converter's noise.
const MIN_LEVEL_DBFS: f32 = -144.0;

/// Largest sample rate accepted, the highest one whose accumulator arithmetic cannot overflow.
///
/// The sine advances its residue by at most `fs/2` from a residue below `fs`, so the sum stays
/// under `1.5 · fs`. Real hardware runs at 48 kHz; this bound exists so that bound is provable
/// rather than obvious.
const MAX_SAMPLE_RATE_HZ: u32 = u32::MAX / 2;

/// Snap a requested sample rate to an integer inside [`MAX_SAMPLE_RATE_HZ`].
///
/// Integer rates are a requirement, not a convenience: the sine's coherence condition needs both
/// frequency and rate integral, and every `describe()` string renders the rate bare.
fn integer_sample_rate(hz: f32) -> Option<u32> {
    if !hz.is_finite() || hz <= 0.0 {
        return None;
    }
    Some((round(f64::from(hz)) as i64).clamp(i64::from(2), i64::from(MAX_SAMPLE_RATE_HZ)) as u32)
}

/// Longest sweep accepted, in samples: ten minutes at 48 kHz.
///
/// Ten minutes is the longest capture this rig plans to take (AC #3's horizon), so a longer
/// stimulus measures nothing extra — and the bound keeps [`ExponentialSweep`]'s peak scan, and
/// the width of the `total_samples` field on the wire, both finite.
const MAX_SWEEP_SAMPLES: u32 = 48_000 * 60 * 10;

/// Machine-readable description of what a source will play.
///
/// Device and host must never disagree about the parameters behind a capture, so the rendering
/// lives with the generator that consumes them. Field order is normative: `name` first so a
/// reader can dispatch before trusting anything else, then the fields shared by every source.
///
/// Rendering goes into a caller-owned buffer and returns the byte count, the same shape the
/// console's record bodies use (`asperitas-logging`'s `boot_body`/`status_body`), because the
/// firmware path has no allocator. Output is space-separated `key=value` fields with no trailing
/// space and no CRLF — the frame supplies the delimiter.
///
/// Real-valued fields carry exactly six fractional digits (`level_dbfs=-20.000000`), always, from
/// one writer rather than from `core`'s float formatter. Why that writer exists, and how many
/// bytes it is worth, is spelled out on `write_decimal`. Integers stay bare.
pub trait Stimulus {
    /// Render the effective parameter set into `out`, returning the bytes written.
    ///
    /// Content longer than `out` is truncated silently, never panicked. Derived quantities
    /// (crest phase, harmonic count, scanned peak scale) stay off the wire: host and device link
    /// this crate, so the algorithm is shared code rather than metadata to negotiate.
    fn describe(&self, out: &mut [u8]) -> usize;
}

/// Scale behind every real-valued `describe()` field: six fractional digits.
///
/// Six digits round-trips an `f32` anywhere these fields reach. The widest real-valued field is a
/// pulse train's `max_frequency_hz` at Nyquist, 24 kHz, where consecutive `f32` values differ by
/// 0.002 - so rounding to a micro-unit can err by 5e-7, some two thousand times less than half an
/// `f32` step there, and a host that re-parses a described value recovers the bits the generator
/// used rather than something near them. That matters here more than it usually does: the point of
/// `describe()` is that the host can reproduce the played waveform, and a description rounded to a
/// tenth of a hertz describes a sweep nobody played.
const DESCRIBE_SCALE: u64 = 1_000_000;

// The zero-padded width in [`write_decimal`] is written as a literal `{:06}`, because `format_args!`
// cannot take a width from a constant. This is what stops the two from drifting apart.
const _: () = assert!(DESCRIBE_SCALE == 1_000_000);

/// Write a value as a decimal with exactly six fractional digits.
///
/// Deliberately not `core`'s float formatter. Printing a float with `{}` pulls in
/// `core::num::flt2dec`, the Dragon/Grisu pair, and the bill is large enough to settle the choice:
/// replacing this function's last line with `{value}` moved the `rig` image (release, `debug = 2`,
/// `log-usb`, measured 2026-09-12) from `.text` 89,804 to 108,172 and `.rodata` 15,808 to 19,116 -
/// 21.4 KB spent to render four numbers, which against the 131,072-byte internal-flash sector
/// would have left 2.5 KB where that binary now has 24. Its cost is also the wrong shape for
/// firmware: those algorithms iterate until a representation short enough to round-trip falls out,
/// so how long they take depends on the bits of the value, while this renders values chosen by
/// whoever armed the stimulus.
///
/// Total over its input domain rather than merely correct on the expected one: non-finite values
/// render as `nan`/`inf`/`-inf`, and a magnitude too large for the scale saturates in the
/// float-to-`u64` conversion instead of wrapping. Neither is reachable from a sanitized parameter;
/// both are defined so that a description can never be the thing that stops the audio path.
fn write_decimal(out: &mut DescWriter, value: f64) {
    if !value.is_finite() {
        let token = if value.is_nan() {
            "nan"
        } else if value > 0.0 {
            "inf"
        } else {
            "-inf"
        };
        let _ = out.write_str(token);
        return;
    }

    let scaled = round(value.abs() * DESCRIBE_SCALE as f64) as u64;
    let negative = value < 0.0;
    let whole = scaled / DESCRIBE_SCALE;
    let frac = scaled % DESCRIBE_SCALE;

    if negative {
        let _ = out.write_str("-");
    }
    let _ = out.write_fmt(format_args!("{whole}.{frac:06}"));
}

/// Render the fields every source shares, owning the separator rule.
///
/// This is the only place the leading grammar is written; each [`Stimulus`] implementation
/// appends its own fields after it.
fn describe_prefix(out: &mut DescWriter, name: &str, sample_rate_hz: u32, level_dbfs: f32) {
    let _ = out.write_fmt(format_args!(
        "name={name} sample_rate_hz={sample_rate_hz} level_dbfs="
    ));
    write_decimal(out, f64::from(level_dbfs));
}

/// A `fmt::Write` that stops at the end of its buffer instead of overflowing it.
///
/// Duplicated from `asperitas-logging`'s `TruncWriter` on purpose: `asperitas-dsp` is a leaf
/// crate that cli, firmware and rig all sit on, so depending on logging to borrow twenty lines
/// would invert the hierarchy for less than the cost of the copy.
struct DescWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> DescWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn filled(&self) -> usize {
        self.pos
    }
}

impl core::fmt::Write for DescWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        let available = self.buf.len() - self.pos;
        let len = s.len().min(available);
        if len == 0 {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + len].copy_from_slice(&s.as_bytes()[..len]);
        self.pos += len;
        Ok(())
    }
}

/// Peak amplitude for a peak-referenced dBFS level: `10^(dbfs/20)`.
///
/// Levels above 0 dBFS clamp to unity. A source that exceeded full scale would be clipped by the
/// output clamp anyway, and a clipped stimulus is not the waveform the analysis reproduces.
fn amplitude_for_dbfs(dbfs: f32) -> (f32, f64) {
    let level = if dbfs.is_finite() {
        dbfs.clamp(MIN_LEVEL_DBFS, 0.0)
    } else {
        MIN_LEVEL_DBFS
    };
    (level, pow(10.0, f64::from(level) / 20.0))
}

/// Greatest common divisor, for the sine's exact-period arithmetic.
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

// ---------------------------------------------------------------------------
// Sine
// ---------------------------------------------------------------------------

/// Parameters for [`Sine`].
#[derive(Clone, Debug)]
pub struct SineParams {
    /// Peak level in dBFS. See the module docs for the convention.
    pub level_dbfs: f32,
    /// Tone frequency in **whole hertz**.
    ///
    /// Integral by requirement, not convenience: coherence needs both the frequency and the
    /// sample rate to be integers, and a `f32` field would promise a resolution the generator
    /// refuses to give. Fractional frequencies belong to the caller, which can pick a window
    /// length instead.
    pub frequency_hz: u32,
}

impl Default for SineParams {
    fn default() -> Self {
        Self {
            level_dbfs: -20.0,
            frequency_hz: 1_000,
        }
    }
}

/// Numerically-controlled oscillator emitting an exact-frequency tone.
///
/// The phase register is an integer residue in `[0, fs)` advanced by whole hertz with one
/// conditional subtract, so `residue[n] = (n · f) mod fs` exactly and no float ever accumulates.
/// There is therefore no drift to bound: the sequence repeats with period `P = fs / gcd(f, fs)`
/// and `out[n + P] == out[n]` bit for bit, forever. This is the NCO shape described by Renesas
/// application note TB318 and the Maine University NCO tutorial — chosen over a per-sample `sin`
/// of a growing float angle (which loses the periodicity) and over a 2-pole resonator (whose
/// pole radius cannot hold unity forever).
///
/// A `u32` DDS register accumulating `round(f · 2^32 / fs)` was the other candidate and is
/// rejected: at 1000 Hz the rounding alone is a 5.6e-8 relative frequency error, within an order
/// of magnitude of the 1e-7 budget, and the residual destroys short-window periodicity.
///
/// ### Why the initial phase is not zero
///
/// Peak amplitude is not free even with an exact NCO. A sample lands on the crest only when the
/// cycle position `n·f/fs` hits a quarter cycle, so with `φ0 = 0` the reported level depends on
/// arithmetic luck: measured at 48 kHz over one period starting at phase 0, 8000 Hz and 16000 Hz
/// peak 1.25 dB low, 4800 Hz and 19200 Hz 0.44 dB low, and 24000 Hz — Nyquist — outputs
/// identically zero.
///
/// Fixing it by scaling the output would put a measured fudge factor in the signal path. Instead
/// the initial phase is placed so that the sample *nearest* the crest lands on the crest: with
/// `k0 = (P + 2) / 4`, the nearest quarter period in units of `1/P`, take `φ0 = π/2 − 2π·k0/P`.
/// Every position `0..P` is reachable because `f/gcd(f, fs)` is coprime with `P`, so some sample
/// sits at exactly `k0/P` and that sample evaluates `sin(π/2)`. Measured peak after this, over one
/// period at 1, 20, 997, 1000, 4800, 8000, 16000, 19200 and 24000 Hz: `1.0` exactly, not
/// approximately. Do not simplify `φ0` back to zero — the numbers above are why it is here.
pub struct Sine {
    sample_rate_hz: u32,
    level_dbfs: f32,
    frequency_hz: u32,
    amplitude: f64,
    period_samples: u32,
    phase_offset: f64,
    residue: u32,
}

impl Default for Sine {
    fn default() -> Self {
        let mut s = Self {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            level_dbfs: 0.0,
            frequency_hz: 0,
            amplitude: 0.0,
            period_samples: 0,
            phase_offset: 0.0,
            residue: 0,
        };
        s.apply(&SineParams::default());
        s
    }
}

impl Processor for Sine {
    type Params = SineParams;

    /// Re-derive the tone for a new rate. The frequency is re-clamped, not just the phase: a
    /// rate lowered without a matching [`Self::set_params`] would otherwise leave a stale
    /// `frequency_hz` above the new Nyquist limit, where the single conditional subtract below
    /// stops being a wrap. Measured before that was fixed: 1 kHz at 48 kHz kept, then dropped to an
    /// 8 kHz rate, emitted a constant 0.1 for 100,000 samples and overflowed the accumulator past
    /// 268,435 of them.
    fn set_sample_rate(&mut self, hz: f32) {
        if let Some(rate) = integer_sample_rate(hz) {
            self.sample_rate_hz = rate;
            self.apply_current();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.apply(params);
    }

    /// Emit the next tone sample on both channels; the input frame is ignored.
    fn tick(&mut self, _input: Frame) -> Frame {
        let angle = core::f64::consts::TAU * f64::from(self.residue)
            / f64::from(self.sample_rate_hz)
            + self.phase_offset;

        // One conditional subtract suffices because `frequency_hz <= fs/2` and `residue < fs`.
        self.residue += self.frequency_hz;
        if self.residue >= self.sample_rate_hz {
            self.residue -= self.sample_rate_hz;
        }

        let v = (self.amplitude * sin(angle)) as f32;
        [v.clamp(-1.0, 1.0), v.clamp(-1.0, 1.0)]
    }

    fn reset(&mut self) {
        self.residue = 0;
    }
}

impl Sine {
    fn apply(&mut self, params: &SineParams) {
        let (level, amplitude) = amplitude_for_dbfs(params.level_dbfs);
        self.level_dbfs = level;
        self.amplitude = amplitude;
        self.frequency_hz = params
            .frequency_hz
            .clamp(1, (self.sample_rate_hz / 2).max(1));
        self.recompute();
    }

    /// Re-apply the effective parameters under the current sample rate.
    fn apply_current(&mut self) {
        let params = SineParams {
            level_dbfs: self.level_dbfs,
            frequency_hz: self.frequency_hz,
        };
        self.apply(&params);
    }

    /// Derive period, crest phase and residue bounds from the current rate and frequency.
    fn recompute(&mut self) {
        let fs = self.sample_rate_hz;
        let period = fs / gcd(self.frequency_hz, fs);
        // Nearest quarter period; ties are symmetric and either side reaches the crest.
        let k0 = (period + 2) / 4;
        self.period_samples = period;
        self.phase_offset = core::f64::consts::FRAC_PI_2
            - core::f64::consts::TAU * f64::from(k0) / f64::from(period);
        self.residue %= fs;
    }

    /// Period of the emitted sequence in samples: `fs / gcd(f, fs)`.
    ///
    /// A coherent analysis window is any whole multiple of this, which is how callers pick a
    /// window that needs no leakage window.
    pub fn period_samples(&self) -> u32 {
        self.period_samples
    }
}

impl Stimulus for Sine {
    fn describe(&self, out: &mut [u8]) -> usize {
        let mut w = DescWriter::new(out);
        describe_prefix(&mut w, "sine", self.sample_rate_hz, self.level_dbfs);
        let _ = w.write_fmt(format_args!(
            " frequency_hz={} period_samples={}",
            self.frequency_hz, self.period_samples
        ));
        w.filled()
    }
}

// ---------------------------------------------------------------------------
// Exponential sweep
// ---------------------------------------------------------------------------

/// Parameters for [`ExponentialSweep`].
#[derive(Clone, Debug)]
pub struct ExponentialSweepParams {
    /// Peak level in dBFS. See the module docs for the convention.
    pub level_dbfs: f32,
    /// Starting frequency in Hz.
    pub f0_hz: f32,
    /// Ending frequency in Hz. Values below `f0_hz` are raised to it.
    pub f1_hz: f32,
    /// Length of the sweep in samples — an exact integer, not a duration.
    ///
    /// Durations invite `(fs · seconds) as usize` truncation, which silently drops or gains a
    /// sample and desynchronises a host that recomputes the waveform from the same numbers.
    pub total_samples: u32,
}

impl Default for ExponentialSweepParams {
    fn default() -> Self {
        Self {
            level_dbfs: -20.0,
            f0_hz: 20.0,
            f1_hz: 20_000.0,
            total_samples: 384_000,
        }
    }
}

/// Exponential sine sweep (ESS) spanning `f0..f1` over an exact sample count.
///
/// Exponential rather than linear: equal energy per octave keeps the recovered impulse response
/// well conditioned, and harmonics of the chirp land *before* the fundamental's arrival, separated
/// in time by order. Order `k` arrives `T · ln(k) / ln(f1/f0)` early — for the defaults, 0.80 s
/// (2nd), 1.27 s (3rd), 1.61 s (4th), 1.86 s (5th) — so one capture yields the linear response
/// and a per-order distortion picture. That is Farina's AES 108th-Convention technique (paper
/// 134); the amplitude pre-equalisation some later treatments discuss is deliberately absent,
/// because pre-emphasis time-smears the recovered response. His AES 122 note also warns against
/// dropping the fade-out, which the end fades provide here; the private `FADE_SAMPLES`
/// constant in this module caps each at 10 ms. Without that fade the default sweep ends at
/// −0.963, effectively a step that smears energy across the spectrum.
///
/// ### Phase comes from the closed-form integral
///
/// `φ(n) = ω0 · T · (r^(n/N) − 1) / ln r` with `r = f1/f0` and `T = N/fs`, evaluated in `f64`.
/// Naively summing instantaneous frequency sample by sample is *not* an equivalent
/// implementation: it differs from this expression by up to 1.31 rad over the default sweep,
/// which is enough to move the deconvolved response. Same reasoning and same expression as
/// `asperitas-cli`'s `synth::generate_sweep`, which backs the golden files.
///
/// Instantaneous frequency at sample `n` is `f0 · r^(n/N)`, so `t = T` is the endpoint *time* and
/// is never emitted: the last sample sits at `f0 · r^((N−1)/N)`, i.e. 19999.64 Hz for the
/// defaults. The wire format carries `f0` and `f1` as documented endpoints, not that value.
///
/// ### Amplitude
///
/// The tapered record is peak-normalised by scanning it once in [`Self::set_params`] and dividing
/// by the largest magnitude found, so `level_dbfs` means peak level for arbitrary endpoints and
/// lengths rather than only for the defaults. For the defaults the scan finds 0.9999999999968866,
/// so it is a no-op there to within 3e-12.
pub struct ExponentialSweep {
    sample_rate_hz: u32,
    level_dbfs: f32,
    f0_hz: f64,
    f1_hz: f64,
    total_samples: u32,
    fade_samples: usize,
    /// `ω0 · T / ln r`, the constant folded out of the phase integral.
    phase_gain: f64,
    /// `f1 / f0`, the chirp ratio.
    ratio: f64,
    /// `amplitude / scanned peak`, so the emitted record peaks at `level_dbfs`.
    scale: f64,
    index: u32,
}

impl Default for ExponentialSweep {
    /// Costs one pass over the default eight-second record (384,000 `sin` evaluations) to find the
    /// peak that makes `level_dbfs` true. Construct once, in setup, not per capture: the scan is
    /// what [`Self::set_params`] does and cannot be deferred into `tick`, where a half-normalised
    /// first sample would be the calibration error this whole module exists to avoid.
    fn default() -> Self {
        let mut s = Self {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            level_dbfs: 0.0,
            f0_hz: 0.0,
            f1_hz: 0.0,
            total_samples: 0,
            fade_samples: 0,
            phase_gain: 0.0,
            ratio: 1.0,
            scale: 0.0,
            index: 0,
        };
        s.apply(&ExponentialSweepParams::default());
        s
    }
}

impl Processor for ExponentialSweep {
    type Params = ExponentialSweepParams;

    fn set_sample_rate(&mut self, hz: f32) {
        let Some(rate) = integer_sample_rate(hz) else {
            return;
        };
        // Re-applying recomputes the envelope *and* rescans the record for its true peak. At the
        // default parameters that is 384,000 sine evaluations, so a caller that sets the rate it
        // already has must not pay for it: every host test constructs `default()` and then sets
        // 48 kHz, and firmware calls this during setup, where a whole-record stall at audio-start
        // would look like a dropped capture.
        if rate == self.sample_rate_hz {
            return;
        }
        self.sample_rate_hz = rate;
        self.apply_current();
    }

    /// Recompute the sweep. Not audio-path cheap: it scans the whole record once to find the
    /// true peak, which for the default eight seconds is ~384k `sin` evaluations (milliseconds).
    /// Call it before arming playback, not from the audio interrupt.
    fn set_params(&mut self, params: &Self::Params) {
        self.apply(params);
    }

    /// Emit the next sweep sample, then silence once the record is exhausted.
    fn tick(&mut self, _input: Frame) -> Frame {
        if self.index >= self.total_samples {
            return [0.0, 0.0];
        }
        let n = self.index as usize;
        self.index += 1;

        let v = (self.scale * self.taper(n) * sin(self.phase(n))) as f32;
        [v.clamp(-1.0, 1.0), v.clamp(-1.0, 1.0)]
    }

    fn reset(&mut self) {
        self.index = 0;
    }
}

impl ExponentialSweep {
    fn apply(&mut self, params: &ExponentialSweepParams) {
        let (level, amplitude) = amplitude_for_dbfs(params.level_dbfs);
        self.level_dbfs = level;

        let nyquist = f64::from(self.sample_rate_hz) / 2.0;
        // Leave room under Nyquist for a chirp that still rises, so `ln r` stays finite.
        let f0 = guarded(params.f0_hz, 20.0)
            .max(1.0)
            .min((nyquist / MIN_CHIRP_RATIO).max(1.0));
        let f1_floor = f0 * MIN_CHIRP_RATIO;
        let f1 = guarded(params.f1_hz, 20_000.0)
            .max(f1_floor)
            .min(nyquist.max(f1_floor));
        self.f0_hz = f0;
        self.f1_hz = f1;
        self.total_samples = params.total_samples.clamp(1, MAX_SWEEP_SAMPLES);
        self.fade_samples = core::cmp::min(FADE_SAMPLES, (self.total_samples as usize) / 2);

        let ratio = f1 / f0;
        let duration_secs = f64::from(self.total_samples) / f64::from(self.sample_rate_hz);
        self.ratio = ratio;
        self.phase_gain = core::f64::consts::TAU * f0 * duration_secs / log(ratio);

        // Peak-normalise by scan: no allocation, and honest for endpoints nobody measured.
        let mut peak = 0.0f64;
        for n in 0..self.total_samples as usize {
            let v = self.taper(n) * sin(self.phase(n));
            peak = peak.max(v.abs());
        }
        self.scale = if peak > 0.0 && peak.is_finite() {
            amplitude / peak
        } else {
            0.0
        };
        self.index = 0;
    }

    fn apply_current(&mut self) {
        let params = ExponentialSweepParams {
            level_dbfs: self.level_dbfs,
            f0_hz: self.f0_hz as f32,
            f1_hz: self.f1_hz as f32,
            total_samples: self.total_samples,
        };
        self.apply(&params);
    }

    /// Closed-form phase integral of the exponential chirp at sample `n`.
    fn phase(&self, n: usize) -> f64 {
        let exponent = n as f64 / f64::from(self.total_samples);
        self.phase_gain * (pow(self.ratio, exponent) - 1.0)
    }

    /// Raised-cosine fade, [`FADE_SAMPLES`] long at both ends, 1.0 in between.
    ///
    /// Counter-driven rather than a [`crate::smooth::Smoother`]: a one-pole has no
    /// reached-target query, so its ramp endpoints are not reproducible sample for sample, and
    /// reproducibility is the entire point of this module.
    ///
    /// `w(j) = 0.5 · (1 − cos(π·j / fade))` gives `w(0) = 0` exactly, so the record begins and
    /// ends on exact zeros and the system sees no step at either boundary.
    fn taper(&self, n: usize) -> f64 {
        let fade = self.fade_samples;
        if fade == 0 {
            return 1.0;
        }
        let from_end = self.total_samples as usize - 1 - n;
        let edge = if n < fade { n } else { from_end };
        if edge >= fade {
            1.0
        } else {
            0.5 * (1.0 - cos(core::f64::consts::PI * edge as f64 / fade as f64))
        }
    }

    /// Instantaneous frequency in Hz at sample index `n`.
    ///
    /// Exposed for tests and for hosts that want to sanity-check a capture's alignment; the
    /// documented endpoints stay on the wire instead (see the type's docs).
    pub fn instantaneous_frequency_hz(&self, n: u32) -> f32 {
        let exponent = f64::from(n) / f64::from(self.total_samples.max(1));
        (self.f0_hz * pow(self.ratio, exponent)) as f32
    }

    /// Number of samples the sweep emits before silence.
    pub fn total_samples(&self) -> u32 {
        self.total_samples
    }
}

impl Stimulus for ExponentialSweep {
    fn describe(&self, out: &mut [u8]) -> usize {
        let mut w = DescWriter::new(out);
        describe_prefix(&mut w, "ess", self.sample_rate_hz, self.level_dbfs);
        let _ = w.write_fmt(format_args!(" f0_hz="));
        write_decimal(&mut w, self.f0_hz);
        let _ = w.write_fmt(format_args!(" f1_hz="));
        write_decimal(&mut w, self.f1_hz);
        let _ = w.write_fmt(format_args!(" total_samples={}", self.total_samples));
        w.filled()
    }
}

/// Fall back to `fallback` for non-finite inputs so degenerate params become ordinary ones.
fn guarded(value: f32, fallback: f64) -> f64 {
    if value.is_finite() {
        f64::from(value)
    } else {
        fallback
    }
}

// ---------------------------------------------------------------------------
// Band-limited pulse train
// ---------------------------------------------------------------------------

/// Parameters for [`PulseTrain`].
#[derive(Clone, Debug)]
pub struct PulseTrainParams {
    /// Peak level in dBFS. See the module docs for the convention.
    pub level_dbfs: f32,
    /// Samples between pulses — the repetition period, and the record length the Dirichlet
    /// kernel is defined over.
    pub period_samples: u32,
    /// Highest frequency the pulse contains, in Hz.
    pub max_frequency_hz: f32,
}

impl Default for PulseTrainParams {
    fn default() -> Self {
        Self {
            level_dbfs: -20.0,
            period_samples: 480,
            max_frequency_hz: 8_000.0,
        }
    }
}

/// Peak-normalized band-limited pulse train.
///
/// A one-sample delta is flat to Nyquist by construction, which puts its energy exactly where an
/// analog loop is least trustworthy and guarantees the measurement is limited by the codec rather
/// than the device. This is the band-limited alternative: the Dirichlet kernel
///
/// `p(m) = sin(π(2K+1)m/N) / ((2K+1) · sin(πm/N))`, `m = n mod N`,
///
/// which is the sum of harmonics `1..=K` of a train repeating every `N` samples. It is exactly
/// periodic, contains nothing above `K · fs/N`, and peaks at exactly 1.0 at `m ≡ 0` **by
/// construction** — no normalisation pass, no measured fudge. Being closed-form and driven by an
/// integer index rather than a float phase accumulator also makes device/host bit identity
/// trivial rather than something to prove. Puckette et al.'s BLIT paper is the prior art for
/// band-limited impulses; the kernel above is derived here rather than quoted because its
/// construction is four lines.
///
/// ### The `K` clamp is load-bearing
///
/// `K` is capped at `(N − 1)/2`, the highest harmonic a period-`N` sequence can carry. Left
/// uncapped, `K → (N − 1)/2` degenerates the kernel toward a delta — measured peak-energy fraction
/// (energy in the crest sample over total energy, period 480) is 0.9979 at `K = 239`, 0.5021 at
/// `K = 120`, and 0.3354 at the default `K = 80`, versus 1.0 for a naive click. Parseval gives
/// that fraction analytically as `(2K + 1)/N`.
///
/// A Hann-windowed sinc would reach lower sidelobes (−31 dB against the Dirichlet's −13.5 dB
/// first sidelobe) at the cost of a wider main lobe and a window expression to keep in sync; the
/// Dirichlet form is the one whose peak normalisation is analytic.
///
/// ### The kernel carries one harmonic of DC
///
/// `D_K = 1 + 2·Σ cos(kx)` includes its constant term, so this pulse is **not** zero-mean. Its
/// offset is `level / (2K + 1)` — 44 dB below the crest at the default `K`, and exactly as loud as
/// the highest retained harmonic in the spectrum. Deconvolution is unaffected, because the host
/// reproduces the same offset from this same source, but an AC-coupled meter reads the pulse
/// quieter than `level_dbfs`, and a caller that subtracts its own DC estimate changes the waveform
/// the analysis expects. Removing the term means giving up the analytic peak normalisation above.
pub struct PulseTrain {
    sample_rate_hz: u32,
    level_dbfs: f32,
    period_samples: u32,
    max_frequency_hz: f64,
    /// Highest harmonic retained: `floor(max_frequency_hz · N / fs)`.
    harmonic_count: u32,
    amplitude: f64,
    position: u32,
}

impl Default for PulseTrain {
    fn default() -> Self {
        let mut s = Self {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            level_dbfs: 0.0,
            period_samples: 0,
            max_frequency_hz: 0.0,
            harmonic_count: 0,
            amplitude: 0.0,
            position: 0,
        };
        s.apply(&PulseTrainParams::default());
        s
    }
}

impl Processor for PulseTrain {
    type Params = PulseTrainParams;

    fn set_sample_rate(&mut self, hz: f32) {
        if let Some(rate) = integer_sample_rate(hz) {
            self.sample_rate_hz = rate;
            self.apply_current();
        }
    }

    fn set_params(&mut self, params: &Self::Params) {
        self.apply(params);
    }

    /// Emit the next pulse sample; the input frame is ignored.
    fn tick(&mut self, _input: Frame) -> Frame {
        let m = self.position;
        self.position = if m + 1 >= self.period_samples {
            0
        } else {
            m + 1
        };

        let period = f64::from(self.period_samples);
        let denom_arg = core::f64::consts::PI * f64::from(m) / period;
        let denom = sin(denom_arg);
        let order = f64::from(2 * self.harmonic_count + 1);
        // Crest: the analytic limit of the kernel at m = 0 is 1.0.
        let normalized = if denom.abs() < CREST_EPSILON {
            1.0
        } else {
            sin(order * denom_arg) / (order * denom)
        };

        let v = (self.amplitude * normalized) as f32;
        [v.clamp(-1.0, 1.0), v.clamp(-1.0, 1.0)]
    }

    fn reset(&mut self) {
        self.position = 0;
    }
}

/// Below this denominator magnitude the kernel is at its crest.
///
/// The smallest non-crest denominator is `sin(π/N)`, which is ≥ 6.5e-11 for any period allowed by
/// [`PulseTrain`]'s clamping, so this catches `m = 0` and nothing else.
const CREST_EPSILON: f64 = 1e-12;

/// Longest repetition period accepted, ~1000 s at 48 kHz. Keeps `sin(π/N)` comfortably above
/// [`CREST_EPSILON`] while leaving far more period than any capture uses.
const MAX_PERIOD_SAMPLES: u32 = 48_000_000;

impl PulseTrain {
    fn apply(&mut self, params: &PulseTrainParams) {
        let (level, amplitude) = amplitude_for_dbfs(params.level_dbfs);
        self.level_dbfs = level;
        self.amplitude = amplitude;
        self.period_samples = params.period_samples.clamp(2, MAX_PERIOD_SAMPLES);

        // `max(lo).min(hi)` rather than `clamp(lo, hi)` for the two f64 bounds below, because both
        // upper bounds are runtime values. `f64::clamp` then keeps its `min > max` panic, whose
        // message formats both bounds with `{:?}`, and that one message links core's whole f64
        // formatter - about 20 KB, which on rig's `stim-pulse` image was the difference between
        // fitting the 128 KB flash and not. Neither panic is reachable (nyquist >= 1 because the
        // sample rate is at least 2, and `k_max.max(1.0)` is at least 1), so the order of `max`
        // then `min` changes no result.
        let nyquist = f64::from(self.sample_rate_hz) / 2.0;
        let max_hz = guarded(params.max_frequency_hz, 8_000.0)
            .max(1.0)
            .min(nyquist);
        self.max_frequency_hz = max_hz;

        let k = floor(max_hz * f64::from(self.period_samples) / f64::from(self.sample_rate_hz));
        let k_max = f64::from((self.period_samples - 1) / 2);
        self.harmonic_count = k.max(1.0).min(k_max.max(1.0)) as u32;
        self.position = 0;
    }

    fn apply_current(&mut self) {
        let params = PulseTrainParams {
            level_dbfs: self.level_dbfs,
            period_samples: self.period_samples,
            max_frequency_hz: self.max_frequency_hz as f32,
        };
        self.apply(&params);
    }

    /// Highest harmonic retained, `floor(max_frequency_hz · N / fs)`.
    pub fn harmonic_count(&self) -> u32 {
        self.harmonic_count
    }
}

impl Stimulus for PulseTrain {
    fn describe(&self, out: &mut [u8]) -> usize {
        let mut w = DescWriter::new(out);
        describe_prefix(&mut w, "pulse_train", self.sample_rate_hz, self.level_dbfs);
        let _ = w.write_fmt(format_args!(
            " period_samples={} max_frequency_hz=",
            self.period_samples
        ));
        write_decimal(&mut w, self.max_frequency_hz);
        w.filled()
    }
}
