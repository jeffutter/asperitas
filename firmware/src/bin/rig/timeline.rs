//! The capture timeline: arm, wait out the window, judge it, dump it.
//!
//! rig arms itself. There is no inbound control channel to arm it through (runtime control is
//! the control channel; the inbound path rig does read carries excerpt installs and nothing else), so the run is a
//! fixed timeline, repeated by resetting the board, which `slow-boot` keeps safe for DFU:
//!
//! ```text
//!   boot      BOOT, RIGCFG, RIGGEN, CAPMAX, then audio starts; a periodic stimulus plays from here,
//!             a one-shot one (the sweep) outputs silence until the first captured callback
//!   +2 s      after the first callback: arm (ARM_DELAY_MS); the first captured callback starts a
//!             one-shot stimulus, so the window holds it from its first sample
//!   +window   disarm; the producer finishes the block it is in, so exactly WINDOW_BLOCKS land
//!             - or earlier, if the ring fills and the producer disarms itself on overrun
//!             A ring-fill build (`ASP_RIG_CAPTURE_SECONDS=ring`) has no window of seconds: that
//!             overrun is how it ends, after exactly RING_BLOCKS, ~349.5 s. Its deadline is only a
//!             backstop (RING_FILL_BACKSTOP_SECONDS past the ring's capacity) for audio that stops.
//!   then      judge the window; a one-shot build checks the stimulus fit and emits STIMSTART; dump
//!             every captured block in ring order, emit DUMPEND
//!   after     idle; CAPSTAT keeps reporting once a second
//! ```
//!
//! The dump waits for the window to close rather than draining alongside it so the measured window
//! carries no bulk USB traffic: worst_gap_us and max_block_us then describe the audio path alone. The
//! gates guarantee the window fits the ring, and a ring fill stops rather than overwrite, so nothing is
//! lost by waiting.
//!
//! Single-shot looks like a missing feature. It is the absence of an inbound control channel, stated
//! here so nobody adds a re-arm path that nothing can trigger.
//!
//! `CAPTURE_ACTIVE` spans arm to `DUMPEND`. An excerpt install is refused for that whole span, and one
//! already open when it begins is abandoned at its next record: see the `install` module.

use crate::dump::dump_ring;
use crate::geometry::{
    ARM_DELAY_MS, CAPTURE_SECONDS, CONTROL_POLL_MS, DEADLINE_SECONDS, RING_FILL, WINDOW_BLOCKS,
};
use crate::report::{judge_capture, report_stimulus_start};
use crate::ring::{Ring, ARMED, AUDIO_STARTED, CAPTURE_ACTIVE, FILLING, PRODUCED};
use asperitas_firmware::audio::{self, TimeBase};
use asperitas_logging::{error, info};
use core::sync::atomic::Ordering;

/// Run the timeline above once, then idle. `one_shot` is the stimulus's `one_shot_samples`.
pub(crate) async fn run_capture(ring: Ring, time_base: TimeBase, one_shot: Option<u32>) {
    while !AUDIO_STARTED.load(Ordering::Relaxed) {
        if audio::AUDIO_EXIT.load(Ordering::Relaxed) != 0 {
            error!("rig: audio never started; nothing to capture");
            return;
        }
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }
    embassy_time::Timer::after_millis(ARM_DELAY_MS).await;

    let first_block = PRODUCED.load(Ordering::Acquire);
    if RING_FILL {
        info!("rig: capture armed until the ring is full ({WINDOW_BLOCKS} blocks)");
    } else {
        info!("rig: capture armed for {CAPTURE_SECONDS} s ({WINDOW_BLOCKS} blocks)");
    }
    CAPTURE_ACTIVE.store(true, Ordering::Release);
    ARMED.store(true, Ordering::Release);

    let deadline =
        embassy_time::Instant::now() + embassy_time::Duration::from_secs(DEADLINE_SECONDS as u64);
    while ARMED.load(Ordering::Acquire) && embassy_time::Instant::now() < deadline {
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }
    ARMED.store(false, Ordering::Release);
    // The producer checks ARMED only at a block boundary, and it runs at P6 above this code, so
    // after the store above it can start no new block: waiting for FILLING to clear waits for the
    // one block in flight, at most 341 ms.
    while FILLING.load(Ordering::Acquire) {
        embassy_time::Timer::after_millis(CONTROL_POLL_MS).await;
    }

    let end_block = PRODUCED.load(Ordering::Acquire);
    info!(
        "rig: capture closed with {} blocks",
        end_block - first_block
    );
    judge_capture(time_base, first_block);
    if let Some(samples) = one_shot {
        report_stimulus_start(first_block, end_block, samples);
    }
    dump_ring(ring, first_block, end_block).await;
    CAPTURE_ACTIVE.store(false, Ordering::Release);
    info!("rig: run complete; reset to capture again");
}
