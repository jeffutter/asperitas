---
id: TASK-026
title: >-
  Fix: Ticker catch-up bursts after an executor stall can defeat button debounce
  and compete with audio
status: In Progress
assignee:
  - '@agent'
created_date: '2026-08-09 05:08'
updated_date: '2026-08-09 05:34'
labels:
  - review-followup
  - planned
dependencies:
  - TASK-025
documentation:
  - docs/reference/daisy-pod.md
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-025 (firmware/src/bin/main.rs:131-137, firmware/src/bin/podtest.rs:156-235, crates/asperitas-pod/src/encoder.rs debounce doc). TASK-025 replaced `Timer::after_millis()` with `embassy_time::Ticker::every()` in both the podtest harness and main.rs's `knob_poll_task` to eliminate steady-state drift (625 Hz vs the documented ~1 kHz contract). That fix is correct for the steady-state case, but embassy-time 0.5.1's `Ticker::next()` (src/timer.rs:271-282) only checks `expires_at <= Instant::now()`; when a caller falls behind, it advances `expires_at` by exactly one `duration` and returns `Ready` immediately — repeating on every subsequent poll until `expires_at` catches up to real time. This means any stall of the cooperative single-threaded embassy executor (a long `controls.poll()`/logging burst, a delayed USB CDC drain, an audio block overrun) causes the *next several* ticks to fire back-to-back with near-zero real elapsed time between them, rather than being throttled the way the old `Timer::after_millis()` code naturally was.

Correctness axis: `crates/asperitas-pod/src/encoder.rs`'s `DEBOUNCE_TICKS = 5` is calibrated assuming ~1 ms of real time between polls ("At 1 kHz poll rate with DEBOUNCE_TICKS = 5, this gives ~5 ms debounce"). A catch-up burst compresses five "consecutive stable readings" into a fraction of a millisecond, so a real contact bounce could pass the debounce filter as a spurious press/release edge right after a stall — exactly the failure mode debounce exists to prevent, and untested today because encoder unit tests drive `DEBOUNCE_TICKS` by call count, not wall time.

Resilience axis, shipped behavior: `firmware/src/bin/main.rs`'s `knob_poll_task` runs on the same executor as the audio DSP path. Previously a late poll self-limited via `Timer::after_millis` computing its deadline from the late `now()`. With `Ticker`, a late poll instead triggers a zero-delay catch-up burst that consumes CPU exactly while the executor is already behind — worsening, not recovering from, an audio overrun.

Corollary (lower confidence, same root cause): `firmware/src/bin/podtest.rs`'s logging goes through a fixed 512-byte non-blocking pipe (`crates/asperitas-logging/src/lib.rs` `LOG_PIPE`, `try_write`, silently drops on overflow). A catch-up burst produces a cluster of log writes in a very short span, which can outpace the USB drain task and drop lines — TASK-018.04's implementation notes already recorded that ~8.8% of log lines are truncated over USB CDC under normal load, so a burst compounds a known-real problem.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The control-surface poll loops in firmware/src/bin/main.rs and firmware/src/bin/podtest.rs bound Ticker catch-up so a stall of N ms produces at most one immediate tick, not a back-to-back burst of missed ticks (e.g. detect an overrun via Instant and call Ticker::reset() or reset_after() to resynchronize instead of letting next() replay the backlog)
- [x] #2 A host-side unit test in crates/asperitas-pod/src/encoder.rs's debounce module demonstrates that DebouncedSwitch does not emit a spurious edge when five 'consecutive' readings are delivered with near-zero elapsed time between them after a simulated stall (construct the scenario with a fake/injected clock or by asserting the bound added in AC #1 at the call-site level, whichever is testable without hardware)
- [x] #3 crates/asperitas-pod/src/encoder.rs's module doc and ControlSurface::poll() doc comment are updated to state the bound now guaranteed, replacing the current unqualified Ticker-fixes-this framing added by TASK-025's fixup
- [x] #4 nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main --release succeeds
- [x] #5 nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main -- -D warnings passes
- [x] #6 nix develop -c cargo test -p asperitas-pod passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approach

Detect Ticker catch-up overruns at each poll loop iteration by tracking wall-clock elapsed time between iterations. When the gap exceeds twice the poll period (indicating the executor stalled and Ticker is about to replay missed ticks), call `Ticker::reset()` to discard the backlog and re-anchor to `now + period`. This bounds any stall to at most one immediate tick rather than an unbounded burst.

## Implementation Steps

### Step 1: Add overrun detection to firmware/src/bin/main.rs (knob_poll_task)

Add named constant and overrun guard before `ticker.next().await`:

```rust
const POLL_INTERVAL_MS: u64 = 1;

// In knob_poll_task, after let mut ticker = ...
let mut last_tick = embassy_time::Instant::now();

loop {
    let (k1, k2) = knobs.read();
    knob_state.write([k1, k2]);

    // Bound catch-up bursts: if executor stalled, reset ticker to discard
    // backlog rather than letting next() fire all missed ticks back-to-back.
    let now = embassy_time::Instant::now();
    if now - last_tick > embassy_time::Duration::from_millis(POLL_INTERVAL_MS * 2) {
        ticker.reset();
    } else {
        ticker.next().await;
    }
    last_tick = now;
}
```

### Step 2: Apply identical pattern to firmware/src/bin/podtest.rs

podtest.rs already has `POLL_INTERVAL_MS` defined. Insert the same overrun guard into the poll\_fut async block, replacing the bare `ticker.next().await` at line 233:

```rust
// Before the loop starts (after color_idx init):
let mut last_tick = embassy_time::Instant::now();

// Replace 'ticker.next().await;' with:
let now = embassy_time::Instant::now();
if now - last_tick > embassy_time::Duration::from_millis(POLL_INTERVAL_MS * 2) {
    ticker.reset();
} else {
    ticker.next().await;
}
last_tick = now;
```

### Step 3: Update doc comments in crates/asperitas-pod/src/encoder.rs

Update the module-level debounce strategy section (lines 25-33) to replace the current warning about Ticker catch-up with documentation of the bound now guaranteed. Update the `ControlSurface::poll()` doc comment (line 198) similarly.

Key message: callers using `Ticker::reset()` on overrun are guaranteed that at most one immediate tick fires after a stall, preserving the temporal spacing assumption of DEBOUNCE_TICKS.

### Step 4: Add host-side test for burst-defeat scenario

In `crates/asperitas-pod/src/encoder.rs` test module, add a test demonstrating that DebouncedSwitch can emit spurious edges when calls are compressed (no wall-clock awareness). This proves why the call-site bound from steps 1-2 is necessary. The test feeds DEBOUNCE_TICKS identical readings in rapid succession and shows an edge is emitted without temporal separation — exactly what the call-site guard prevents.

### Step 5: Verify build and tests

- `nix develop -c cargo test -p asperitas-pod`
- `nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main --release`
- `nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main -- -D warnings`

## Why no sub-tickets

All changes implement a single mechanism (overrun detection + reset) applied to two call sites plus supporting docs/tests. The work is <50 lines across 3 files and must ship atomically.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC #1: Added overrun detection with Ticker::reset() to both main.rs knob_poll_task and podtest.rs poll loop. When gap between iterations exceeds 2× poll period (2 ms), ticker is reset to discard backlog instead of replaying missed ticks. AC #2: Added burst_of_identical_readings_emits_edge_without_wall_clock test proving DebouncedSwitch emits on call count alone. AC #3: Updated module doc and ControlSurface::poll() doc to state the bound now guaranteed.
<!-- SECTION:NOTES:END -->
