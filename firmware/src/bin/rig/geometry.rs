//! Capture geometry and rate gates: which lane, how long a window, and the compile-time asserts
//! that tie them to the ring and the audio period.

use asperitas_wire::capture;
use asperitas_wire::console;

/// Which input lane the ring keeps.
///
/// Left, matching `src/bin/main.rs`'s `decode_block`, which reads `words[0]` as left from the same
/// interleaved layout. Which physical jack that is on the Pod is a bench observation to make
/// at the bench; if it is the wrong one, flipping this constant is the whole fix, because
/// [`MONO_WORD`] and `RIGCFG`'s `lane` both derive from it.
pub(crate) const MONO_LANE: console::MonoLane = console::MonoLane::Left;

/// The interleaved word offset [`MONO_LANE`] names.
pub(crate) const MONO_WORD: usize = match MONO_LANE {
    console::MonoLane::Left => 0,
    console::MonoLane::Right => 1,
};

/// What closes a capture window.
#[derive(Clone, Copy)]
enum Window {
    /// A deadline this many seconds after arming.
    Seconds(usize),
    /// No deadline of its own: the window runs until the producer finds no `Free` block and
    /// disarms itself, so it holds exactly the whole ring. This is the run that checks `CAPMAX`'s
    /// `seconds_max` against the wall clock.
    RingFill,
}

/// The window this build records, from `ASP_RIG_CAPTURE_SECONDS` when set.
///
/// The override exists so a bench run can change its window without editing source
/// (`ASP_RIG_CAPTURE_SECONDS=30 cargo build ...`, or `=ring` to fill the ring); rustc tracks the
/// variable, so changing it rebuilds. A number of seconds is held to the ring gate below, so it can
/// shorten the window freely and lengthen it only as far as the ring allows.
const WINDOW: Window = match option_env!("ASP_RIG_CAPTURE_SECONDS") {
    Some(text) => parse_window(text),
    None => Window::Seconds(capture::CAPTURE_WINDOW_SECONDS),
};

/// Parse the window at compile time: a whole number of seconds, or the word `ring`. Anything else
/// fails the build, naming the variable, instead of falling back to a default nobody asked for.
const fn parse_window(text: &str) -> Window {
    let bytes = text.as_bytes();
    assert!(
        !bytes.is_empty(),
        "ASP_RIG_CAPTURE_SECONDS is set but empty"
    );
    if let [b'r', b'i', b'n', b'g'] = bytes {
        return Window::RingFill;
    }
    let mut seconds = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i].is_ascii_digit(),
            "ASP_RIG_CAPTURE_SECONDS must be a whole number of seconds, or `ring`"
        );
        seconds = seconds * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    Window::Seconds(seconds)
}

// The parser's arms, checked where they are compiled. A value that fails to parse cannot be tested
// this way - it fails the build, which is the behaviour - so these pin the arms that must succeed.
const _: () = assert!(matches!(parse_window("ring"), Window::RingFill));
const _: () = assert!(matches!(parse_window("300"), Window::Seconds(300)));
const _: () = assert!(matches!(parse_window("0"), Window::Seconds(0)));

/// Whether this build fills the ring rather than closing on a deadline.
pub(crate) const RING_FILL: bool = matches!(WINDOW, Window::RingFill);

/// Seconds of capture this build records: the window's own figure, or for a ring fill the ring's
/// floored capacity (`CAPMAX`'s `seconds_max`, 349), which is what `RIGCFG`'s `window_s` reports.
pub(crate) const CAPTURE_SECONDS: usize = match WINDOW {
    Window::Seconds(seconds) => seconds,
    Window::RingFill => capture::ring_seconds_floor() as usize,
};

/// Ring blocks the window consumes: 879 at the default 300 s. Rounded up by `expected_blocks`,
/// because the producer always finishes the block it is in when the window closes. A ring fill
/// consumes every block.
pub(crate) const WINDOW_BLOCKS: usize = match WINDOW {
    Window::Seconds(seconds) => capture::expected_blocks(seconds),
    Window::RingFill => capture::RING_BLOCKS,
};

/// Bytes the window needs, as `CAPMAX`'s `total_bytes` reports them.
pub(crate) const WINDOW_BYTES: usize = match WINDOW {
    Window::Seconds(seconds) => seconds * capture::BYTES_PER_SECOND,
    Window::RingFill => capture::RING_BYTES,
};

/// Backstop past a ring fill's expected end, in seconds. The producer's disarm is what closes a
/// ring fill; this deadline only fires if it never comes (audio stopped mid-window), so the run still
/// judges and dumps what it holds instead of waiting forever, and the judge reports the shortfall.
const RING_FILL_BACKSTOP_SECONDS: usize = 10;

/// Seconds after arming at which the timeline closes the window if nothing closed it sooner.
pub(crate) const DEADLINE_SECONDS: usize = if RING_FILL {
    // Floored capacity plus the backstop: past the 349.5 s the ring actually takes to fill.
    CAPTURE_SECONDS + RING_FILL_BACKSTOP_SECONDS
} else {
    CAPTURE_SECONDS
};

/// One audio period in whole microseconds, floored: 666 (32 frames at 48 kHz is 666.67 us).
///
/// Floored because it is used as a deadline, and a deadline rounded up admits a callback that
/// actually missed it.
const PERIOD_US: u32 =
    (capture::FRAMES_PER_CALLBACK as u64 * 1_000_000 / capture::SAMPLE_RATE_HZ as u64) as u32;

/// Longest a callback may run: one period. Past it the callback is still working when the DMA
/// wants the next half-buffer, which is the definition of an underrun rather than a slow one.
pub(crate) const CALLBACK_BUDGET_US: u32 = PERIOD_US;

/// Longest gap between callback starts that still counts as on time: one period plus a quarter.
///
/// The slack absorbs interrupt-entry jitter - callback N entered late, N+1 on time - which stretches
/// one gap and shrinks the next without losing a sample. A quarter period is far more jitter than
/// the P6 executor should ever see and far less than a missed callback, which is what the gate
/// below pins.
pub(crate) const GAP_LIMIT_US: u32 = PERIOD_US + PERIOD_US / 4;

// Rate gates. The geometric ones are compile-time facts and fail the build. The three that judge a
// measurement (`worst_gap_us`, `max_block_us`, delivered against expected) can only be judged once a
// window has run; `judge_capture` reports each beside the number it judges, and the asserts here pin
// the thresholds those judgments use so they cannot drift into meaninglessness.
const _: () = assert!(
    CAPTURE_SECONDS > 0,
    "a zero-second capture window records nothing"
);
// A window of seconds must fit the ring, because its deadline is what ends it. A ring fill is ended
// by the ring itself, so it is exempt, and its backstop must not close it before the ring is full.
const _: () = assert!(
    RING_FILL || WINDOW_BLOCKS < capture::RING_BLOCKS,
    "the capture window does not fit the ring; shorten ASP_RIG_CAPTURE_SECONDS"
);
const _: () = assert!(
    !RING_FILL || DEADLINE_SECONDS as u64 * 1_000_000 > capture::ring_duration_micros(),
    "a ring fill's backstop deadline would close the window before the ring is full"
);
const _: () = assert!(
    capture::RING_BYTES <= daisy_embassy::sdram::SDRAM_SIZE,
    "the capture ring is larger than the SDRAM it lives in"
);
const _: () = assert!(
    GAP_LIMIT_US > PERIOD_US && GAP_LIMIT_US < 2 * PERIOD_US,
    "the gap limit must admit jitter and still catch one missed callback"
);
const _: () = assert!(
    CALLBACK_BUDGET_US <= PERIOD_US,
    "a callback budget past one period admits underruns"
);

/// How often `report_capstat` emits, gated below against the dump it shares the pipe with.
pub(crate) const CAPSTAT_PERIOD_MS: usize = 1_000;

/// Milliseconds one ring block takes to fill: 341.
const BLOCK_FILL_MS: usize = capture::callbacks_per_block() * capture::FRAMES_PER_CALLBACK * 1_000
    / capture::SAMPLE_RATE_HZ as usize;

/// Most `CAPSTAT` records that can land while one block fills, plus one for phase slip: 2.
const CAPSTAT_MAX_PER_BLOCK: usize = BLOCK_FILL_MS / CAPSTAT_PERIOD_MS + 2;

// Status traffic cannot dominate the dump: worst-case CAPSTAT bytes per block stay under one
// percent of the wire bytes that block costs to dump (2 x 200 x 100 < 57 788). Relative on
// purpose, because the link ceiling is unmeasured; both sides are constants the crate that renders
// them owns and host-tests, not numbers typed here.
const _: () = assert!(
    CAPSTAT_MAX_PER_BLOCK * console::CAPSTAT_MAX_BODY * 100 < capture::wire_bytes_per_block(),
    "CAPSTAT traffic would exceed one percent of the dump's own record traffic"
);

/// Delay from the first audio callback to arming.
///
/// Long enough for `BOOT`, `RIGCFG`, `RIGGEN` and `CAPMAX` to have drained to an attached host, so
/// the description of a capture always precedes it, and short enough not to matter to a person
/// waiting at the bench.
pub(crate) const ARM_DELAY_MS: u64 = 2_000;

/// How often the capture timeline re-checks for an early stop.
pub(crate) const CONTROL_POLL_MS: u64 = 100;

/// How long the dump writer yields after a refused chunk. Short, because a refusal clears as soon
/// as the drain has moved one packet; the point of the wait is the yield, not the duration.
#[cfg(feature = "log-usb")]
pub(crate) const DUMP_RETRY_MS: u64 = 1;
