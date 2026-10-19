//! Capture counters and the records that describe them: `CAPMAX`, `CAPSTAT`, the window verdicts
//! and `STIMSTART`.

use crate::emit_console;
use crate::geometry::{
    CALLBACK_BUDGET_US, CAPSTAT_PERIOD_MS, GAP_LIMIT_US, RING_FILL, WINDOW_BLOCKS, WINDOW_BYTES,
};
use crate::ring::{
    DUMPED, FILL_ENDED_MS, FILL_STARTED_MS, INVALID_GAPS, MAX_BLOCK_CYCLES, OVERRUN, PRODUCED,
    STIM_BLOCK, STIM_CALLBACK, STIM_STARTED, WORST_GAP_CYCLES,
};
use asperitas_firmware::audio::{self, TimeBase};
use asperitas_logging::{error, info};
use asperitas_wire::capture;
use asperitas_wire::console;
use core::sync::atomic::Ordering;

/// The `embassy_time` clock in whole milliseconds, truncated to the width the fill stamps keep.
pub(crate) fn now_ms() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}

/// Raw cycles to whole microseconds, with the time base boot settled on.
fn cycles_to_us(cycles: u32, time_base: TimeBase) -> u32 {
    cycles / time_base.cycles_per_us
}

/// The one `CAPMAX` record: what this build's ring could hold, all from `capture::` arithmetic.
pub(crate) fn emit_capmax() {
    let cap = console::RingCapacity {
        total_bytes: WINDOW_BYTES as u32,
        ring_bytes: capture::RING_BYTES as u32,
        seconds_max: capture::ring_seconds_floor(),
        us_max: capture::ring_duration_micros() as u32,
        // Block-granular, because the ring is consumed in whole blocks: these are the bytes no
        // block of this window will ever touch.
        unused_headroom_bytes: ((capture::RING_BLOCKS - WINDOW_BLOCKS) * capture::RING_BLOCK_BYTES)
            as u32,
    };
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::capmax_body(&cap, &mut body);
    emit_console(&body[..n]);
}

/// The capture counters as they stand, in `CAPSTAT`'s shape.
fn capture_status(time_base: TimeBase) -> console::CaptureStatus {
    let delivered = PRODUCED.load(Ordering::Acquire);
    console::CaptureStatus {
        delivered,
        // Counting down, as the field is defined: blocks this window still owes. Zero at the end of
        // a window that delivered everything.
        expected: (WINDOW_BLOCKS as u32).saturating_sub(delivered),
        overrun: OVERRUN.load(Ordering::Relaxed),
        max_block_us: cycles_to_us(MAX_BLOCK_CYCLES.load(Ordering::Relaxed), time_base),
        worst_gap_us: cycles_to_us(WORST_GAP_CYCLES.load(Ordering::Relaxed), time_base),
        audio_exit: audio::AUDIO_EXIT.load(Ordering::Relaxed),
        dumped: DUMPED.load(Ordering::Relaxed),
        dropped_full: console::CONSOLE.snapshot().dropped_full,
    }
}

/// Emit `CAPSTAT` about once a second, forever, reading atomics only.
///
/// Cheap on purpose: it shares the log pipe with the dump, and a dump only ever gets what the
/// reserve leaves. If this cadence ever crowds the dump, `DUMPEND`'s `dropped_full` and `refused`
/// climb, and the period here is the first knob to turn.
pub(crate) async fn report_capstat(time_base: TimeBase) {
    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_millis(
        CAPSTAT_PERIOD_MS as u64,
    ));
    #[cfg(feature = "log-usb")]
    let mut body = [0u8; console::BODY_WINDOW];
    loop {
        ticker.next().await;
        let st = capture_status(time_base);
        #[cfg(feature = "log-usb")]
        {
            let n = console::capstat_body(&st, &mut body);
            emit_console(&body[..n]);
        }
        // No console means no CAPSTAT record, but the starvation signals are still worth a probe
        // reader's time, so the same facts go out as an ordinary log line.
        #[cfg(not(feature = "log-usb"))]
        info!(
            "rig: capstat delivered={} expected={} overrun={} max_block_us={} worst_gap_us={} audio_exit={} dumped={} dropped_full={}",
            st.delivered,
            st.expected,
            st.overrun,
            st.max_block_us,
            st.worst_gap_us,
            st.audio_exit,
            st.dumped,
            st.dropped_full,
        );
    }
}

/// Judge the window that just closed against the runtime half of the rate gates, each beside the
/// number it judges.
pub(crate) fn judge_capture(time_base: TimeBase, first_block: u32) {
    let st = capture_status(time_base);
    let delivered = st.delivered - first_block;
    let verdict = |ok: bool| if ok { "pass" } else { "FAIL" };
    info!(
        "rig: gate delivered {delivered} == expected {WINDOW_BLOCKS}: {}",
        verdict(delivered as usize == WINDOW_BLOCKS)
    );
    info!(
        "rig: gate worst_gap_us {} < {GAP_LIMIT_US}: {} ({} gaps too wide to measure)",
        st.worst_gap_us,
        verdict(st.worst_gap_us < GAP_LIMIT_US),
        INVALID_GAPS.load(Ordering::Relaxed),
    );
    info!(
        "rig: gate max_block_us {} < {CALLBACK_BUDGET_US}: {}",
        st.max_block_us,
        verdict(st.max_block_us < CALLBACK_BUDGET_US)
    );
    if RING_FILL {
        // The one overrun is the full ring refusing the next claim: the window's terminator, not a
        // loss. Zero means the backstop deadline closed the window instead, and the ring never filled.
        info!(
            "rig: gate overrun {} == 1 (the full ring ending the window, expected): {}",
            st.overrun,
            verdict(st.overrun == 1)
        );
        let started = FILL_STARTED_MS.load(Ordering::Relaxed);
        let ended = FILL_ENDED_MS.load(Ordering::Relaxed);
        if started != 0 && ended != 0 {
            info!(
                "rig: ring filled in {} ms, first armed callback to disarm (CAPMAX seconds_max {}, us_max {})",
                ended.wrapping_sub(started),
                capture::ring_seconds_floor(),
                capture::ring_duration_micros(),
            );
        } else {
            error!(
                "rig: ring fill not timed (started_ms={started}, ended_ms={ended}); the ring never filled: FAIL"
            );
        }
    } else {
        info!(
            "rig: gate overrun {} == 0: {}",
            st.overrun,
            verdict(st.overrun == 0)
        );
    }
}

/// One-shot builds: check the stimulus played whole inside the window just closed, and emit
/// `STIMSTART` saying where it began. `samples` is the stimulus's length from `one_shot_samples`.
///
/// Says nothing rather than something wrong: no `STIMSTART` when the stimulus never started (the
/// window captured nothing) or started somewhere the window does not contain, which the start rule
/// in the audio callback makes unreachable and this reports rather than trusts.
pub(crate) fn report_stimulus_start(first_block: u32, end_block: u32, samples: u32) {
    if !STIM_STARTED.load(Ordering::Acquire) {
        error!("rig: the one-shot stimulus never started; no STIMSTART");
        return;
    }
    let block = STIM_BLOCK.load(Ordering::Relaxed);
    let callback = STIM_CALLBACK.load(Ordering::Relaxed) as usize;
    let Some(offset) = capture::window_sample_offset(first_block, block, callback) else {
        error!(
            "rig: stimulus started at block {block} callback {callback}, outside the window from block {first_block}; no STIMSTART"
        );
        return;
    };
    let captured = u64::from(end_block - first_block) * capture::samples_per_block() as u64;
    let end = u64::from(offset) + u64::from(samples);
    let verdict = if end <= captured { "pass" } else { "FAIL" };
    info!(
        "rig: gate stimulus samples {offset}..{end} inside the {captured}-sample capture: {verdict}"
    );
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::stimstart_body(
        &console::StimulusStart {
            first_block,
            offset,
        },
        &mut body,
    );
    emit_console(&body[..n]);
}
