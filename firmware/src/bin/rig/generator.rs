//! The stimulus generator this build plays, chosen by cargo feature, and the codec rate it renders at.

#[cfg(feature = "stim-excerpt")]
use crate::excerpt_play::ExcerptPlayer;
use asperitas_firmware::audio::BLOCK_LENGTH;
use asperitas_stimulus::fx_chain::FxChain;
#[cfg(feature = "stim-ess")]
use asperitas_stimulus::stimulus::ExponentialSweep;
#[cfg(feature = "stim-pulse")]
use asperitas_stimulus::stimulus::PulseTrain;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse", feature = "stim-excerpt")))]
use asperitas_stimulus::stimulus::Sine;
use asperitas_wire::capture;
use daisy_embassy::audio::Fs;
use static_cell::StaticCell;
/// How many of the four generators a build selected. Zero is a valid answer and means sine.
const GENERATORS_SELECTED: usize = {
    // Written as a const block rather than `usize::from(cfg!(..))` sums: `From<bool>` is not
    // const-callable on this toolchain, so the conversion has to happen in plain control flow.
    let mut selected = 0;
    if cfg!(feature = "stim-sine") {
        selected += 1;
    }
    if cfg!(feature = "stim-ess") {
        selected += 1;
    }
    if cfg!(feature = "stim-pulse") {
        selected += 1;
    }
    if cfg!(feature = "stim-excerpt") {
        selected += 1;
    }
    selected
};

/// Two generators in one image is a mistake, not a mode: there is no inbound console channel
/// that could say which one the capture used, so the bytes would arrive describable only by
/// guessing. Reject it where the build can still hear about it.
///
/// This is also why `stim-sine` is not in `default`. A default sine plus `--features
/// seed3,stim-ess` would select two generators and trip this very assert.
///
/// Written `< 2` rather than `<= 1`: in a build that selects no generator the count is 0, and
/// clippy reads a comparison against 1 as one against the type's minimum and calls it vacuous
/// (`absurd_extreme_comparisons`). Same bound, and the count is still what a human should read.
const _: () = assert!(
    GENERATORS_SELECTED < 2,
    "select at most one stimulus: stim-sine, stim-ess, stim-pulse or stim-excerpt"
);

/// The generator this build plays.
///
/// The sine arm's `cfg` covers both spellings of "sine": explicitly `--features stim-sine`, and
/// no stimulus feature at all. Mutual exclusion above is what makes leaving `stim-sine` out of
/// that condition correct rather than a subtle wrong-build generator.
#[cfg(feature = "stim-ess")]
pub(crate) type BareGenerator = ExponentialSweep;
#[cfg(feature = "stim-pulse")]
pub(crate) type BareGenerator = PulseTrain;
#[cfg(feature = "stim-excerpt")]
pub(crate) type BareGenerator = ExcerptPlayer;
#[cfg(not(any(feature = "stim-ess", feature = "stim-pulse", feature = "stim-excerpt")))]
pub(crate) type BareGenerator = Sine;

/// What the audio task holds: the generator, then the replay effect (`CTL FX`), on every build.
/// With no `FX` sent the effect is bypass and the output is bit-identical to the bare generator
/// (`asperitas_stimulus::fx_chain`).
pub(crate) type ActiveGenerator = FxChain<BareGenerator>;

/// The chain around `gen`: shipped smoothing, or none under `unsmoothed-params`.
pub(crate) fn chain_of(gen: BareGenerator) -> ActiveGenerator {
    #[cfg(feature = "unsmoothed-params")]
    {
        FxChain::new_unsmoothed(gen)
    }
    #[cfg(not(feature = "unsmoothed-params"))]
    {
        FxChain::new(gen)
    }
}

/// Sample rate rig renders at, derived from the capture format rather than restated beside it.
///
/// The capture geometry in `asperitas-logging::capture` prices itself at 48 kHz mono 16-bit, and the
/// codec runs at whatever [`RIG_FS`] says. If those ever disagree the measurement is nonsense, so
/// one of them has to be the source of truth; the capture format is the one the host parses, and
/// the assert under [`RIG_FS`] holds the codec to it.
pub(crate) const SAMPLE_RATE_HZ: f32 = capture::SAMPLE_RATE_HZ as f32;

/// The codec rate, named rather than left to `AudioConfig::default()` so the build can check it.
pub(crate) const RIG_FS: Fs = Fs::Fs48000;

/// `Fs` as hertz. daisy-embassy keeps its own table private inside `into_clock_divider`, which also
/// reads the live SAI clock and so cannot run in const context; this is that table's first half.
const fn fs_hz(fs: Fs) -> u32 {
    match fs {
        Fs::Fs8000 => 8_000,
        Fs::Fs32000 => 32_000,
        Fs::Fs44100 => 44_100,
        Fs::Fs48000 => 48_000,
        Fs::Fs88200 => 88_200,
        Fs::Fs96000 => 96_000,
    }
}

// `capture` copies two driver facts it cannot name without pulling the HAL into a host-tested crate.
// This is the side that can see both, so the loop closes here.
const _: () = assert!(
    fs_hz(RIG_FS) == capture::SAMPLE_RATE_HZ,
    "the codec rate and the capture format disagree"
);
// Compared in samples, deliberately. `capture::CALLBACK_BYTES` (64 bytes of one mono i16 lane) and
// `daisy_embassy::audio::HALF_DMA_BUFFER_LENGTH` (64 u32 words of interleaved stereo) are equal as
// numbers and unrelated as quantities: one is what the ring gains per callback, the other is what
// the DMA moves. A gate written against those two would keep passing after either side changed
// shape, so it is written against the one quantity both sides mean the same thing by.
const _: () = assert!(
    capture::FRAMES_PER_CALLBACK == BLOCK_LENGTH,
    "capture::FRAMES_PER_CALLBACK no longer matches the driver's samples per callback"
);

/// Storage for the one generator instance.
pub(crate) static GENERATOR: StaticCell<ActiveGenerator> = StaticCell::new();
