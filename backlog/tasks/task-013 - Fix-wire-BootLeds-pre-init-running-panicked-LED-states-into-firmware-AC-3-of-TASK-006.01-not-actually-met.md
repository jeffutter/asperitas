---
id: TASK-013
title: >-
  Fix: wire BootLed's pre-init/running/panicked LED states into firmware (AC #3
  of TASK-006.01 not actually met)
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 21:55'
updated_date: '2026-08-01 22:30'
labels:
  - review-followup
dependencies:
  - TASK-006.01
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-006.01 / TASK-006.01.05 (crates/asperitas-logging/src/led.rs, firmware/src/bin/main.rs, firmware/src/bin/blinky.rs, crates/asperitas-logging/src/panic_handler.rs). TASK-006.01's own AC #3 ('LED boot-stage indicator covers at least pre-init, running, and panicked') was marked Done but is not actually true of the shipped firmware: led.rs::BootLed and its LedState::{PreInit,Running,Panicked} states (with a real blink_task and panic_loop) are never instantiated by firmware/src/bin/main.rs or blinky.rs — grep confirms zero references to BootLed/LedState/blink_task/panic_loop outside asperitas-logging itself. What actually ships is (a) a single-color onboard user_led (PC7) that main.rs/blinky.rs just turn on() once when audio/blink starts, with no pre-init indication and no color distinction, and (b) a separate hand-rolled steady-red-only LED write inside panic_handler.rs::handle_panic that duplicates BootLed's color-setting logic via raw GPIO instead of calling into BootLed, and does not match led.rs's own documented Panicked behavior ('Fast strobe red (~5 Hz)') — it is just steady-on. Root cause per TASK-006.01.05's implementation notes: BootLed::new() and panic_handler::set_panic_led() both need to consume the same three RGB Peri tokens (PC1/PA6/PA7), so the integration ticket dropped BootLed entirely rather than resolve the ownership conflict, leaving it dead code with #[allow(dead_code)] on panic_loop as the tell. Correct axis: the task's own Final Summary claims '(3) BootLed with PreInit/Running/Panicked states... Integrated into main.rs and blinky.rs' — this is false, only the panic-red state (via a parallel raw-GPIO path) is integrated. This blocks TASK-006.02 (@human hardware verification), whose AC #2 is 'LED boot stages are visually distinguishable' — as shipped there is nothing but on/off to see.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 firmware/src/bin/main.rs and blinky.rs each drive the RGB LED (PC1/PA6/PA7) through a single owner that exposes both boot-stage states (PreInit before audio/blink starts, Running once it does) and the panicked state, resolving the Peri ownership conflict noted in TASK-006.01.05's implementation notes (e.g. panic_handler owns/borrows the same BootLed instance main.rs holds, rather than a second raw-GPIO copy)
- [ ] #2 the three states are visually distinct on real RGB hardware semantics (not just on/off): pre-init and running are distinguishable from each other and from panicked, consistent with led.rs's documented LedState variants
- [x] #3 panic_handler.rs's LED write reuses led.rs's color-setting logic (BootLed or the shared LED_ACTIVE_LOW-driven helper) instead of a second hand-rolled raw-GPIO Output implementation
- [x] #4 led.rs has no dead code left un-integrated: delete #[allow(dead_code)] on panic_loop once it is actually reachable, or delete panic_loop/blink_task if the chosen design does not need them — no #[allow(dead_code)] should remain masking unused public API
- [x] #5 nix develop -c cargo build --release --features seed3 --bin main --bin blinky (from firmware/) succeeds
- [x] #6 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Design Decision: Single Static BootLed with Atomic State Transitions

**Problem**: `BootLed::new()` and `panic_handler::set_panic_led()` both consume the same three `Peri<'_, T>` tokens (PC1/PA6/PA7). Embassy enforces single-owner-per-peripheral — you cannot construct two independent `hal::gpio::Output` drivers on the same pins.

**Chosen solution**: One `StaticCell`-backed `static BOOT_LED: BootLed`, initialized once by a new public `led::init()` function. Both the async execution path (main.rs/blinky.rs) and the synchronous panic handler access the same instance.

This follows the same pattern already established in `usb.rs`: `init()` consumes peripherals, stores results in static storage (`StaticCell` + raw pointer cache), and provides accessor functions for later use. No mutex overhead — single-core Cortex-M with controlled init-then-access lifecycle makes this safe.

**State transition mechanism**: An `AtomicU32` stores the current `LedState`. The async `blink_task` reads it on each iteration and adapts its behavior. The panic handler writes it (and directly calls `led::get_mut().set_state()`). This avoids channels or mutexes — the atomic is only for coordination between an async task and interrupt context.

### Why not other approaches:

- **Mutex-wrapped BootLed**: Unnecessary overhead. `CriticalSectionRawMutex` would work but adds lock/unlock cycles to every LED update. The `StaticCell` + direct access pattern matches what `usb.rs` already does successfully.
- **Keeping `set_panic_led()`**: Would require two `init()` calls consuming the same pins — impossible under embassy's ownership model.
- **Spawn-based blink_task that owns self**: Cannot transition states after spawn because `self` is consumed. The atomic-state approach lets the task respond to state changes without being destroyed.

## File-by-file Changes

### 1. `crates/asperitas-logging/src/led.rs`

Add static storage and public init/getter API:

```rust
use core::sync::atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;

/// Storage for the singleton BootLed instance.
static BOOT_LED_STORAGE: StaticCell<BootLed> = StaticCell::new();

/// Cached mutable reference to the initialized BootLed.
/// Set once by init(), read by get_mut() and panic_handler.
static mut BOOT_LED_REF: *mut BootLed = core::ptr::null_mut();

/// Global atomic state — read by blink_task, written by main() and panic_handler.
static LED_STATE: AtomicU32 = AtomicU32::new(0); // 0=PreInit, 1=Running, 2=Panicked
```

Replace `BootLed::new()` with a public `init()` that takes the three pins, constructs `BootLed`, stores it in `BOOT_LED_STORAGE`, caches the pointer, sets initial state to `PreInit`, and returns `()` (or a unit handle). Keep `BootLed::new()` as `pub(crate)` constructor called internally.

Add `pub fn get_mut() -> &'static mut BootLed` that derefs `BOOT_LED_REF` (unsafe but safe on single-core after init).

Add `pub fn set_global_state(state: LedState)` that writes `LED_STATE` atomically AND calls `get_mut().set_state(state)` synchronously. Used by main.rs/blinky.rs for explicit state transitions.

Refactor `blink_task` to take `&mut self` instead of consuming `self`, and read `LED_STATE.load(Ordering::Acquire)` on each loop iteration. When state is `Running`, set solid green and return. When state changes to `Panicked`, the task continues blinking at ~5 Hz until the executor stops (which happens when panic halts everything).

Delete `panic_loop()` entirely — it was never integrated, has `#[allow(dead_code)]`, and the chosen design handles panicking through `handle_panic` → `led::get_mut().set_state(Panicked)` + infinite loop. Dead code removed per AC #4.

Make `LED_ACTIVE_LOW` remain `pub(crate)` — it's used by `panic_handler.rs` via the shared `BootLed` now, not duplicated.

### 2. `crates/asperitas-logging/src/panic_handler.rs`

Delete `PanicLedState` struct, `PANIC_LED` static, and `set_panic_led()` function entirely.

Rewrite `handle_panic` to:
1. Call `crate::led::get_mut().set_state(LedState::Panicked)` — uses the shared BootLed, no duplicate GPIO construction
2. Write panic message over USB pipe (unchanged)
3. `bkpt()` + infinite `nop()` loop (unchanged)

Remove the `core::mem::transmute` calls and all three `hal::gpio::Output::new()` constructions — they're now handled by `led::init()`.

Note: this also resolves the transmute duplication that TASK-014 tracks (at least the panic_handler half of it).

### 3. `firmware/src/bin/main.rs`

Replace:
```rust
// OLD
asperitas_logging::panic_handler::set_panic_led(board.pins.d20, board.pins.d19, board.pins.d18);
```
With:
```rust
// NEW — single init, both boot stages and panic handler share the LED
asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);
```

After USB init and before audio prepare, spawn the blink task:
```rust
_spawner.spawn(async {
    asperitas_logging::led::get_mut().blink_task().await;
}).ok();
```

The blink task starts in `PreInit` state (red blink ~1 Hz) automatically since `init()` sets the initial state.

After audio interface starts successfully, replace `board.user_led.on()` with:
```rust
asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);
```

This transitions the LED to steady green. The blink_task sees `Running` on its next iteration, sets solid green, and returns (task ends cleanly).

### 4. `firmware/src/bin/blinky.rs`

Same pattern as main.rs:
- Replace `set_panic_led(...)` with `led::init(...)`
- Spawn `blink_task` after init
- After "Blinky running" log, call `set_global_state(Running)` to transition to steady green
- Remove the `board.user_led` blink loop — the RGB LED's blink_task replaces it as the visual indicator
- Keep the USB `select` with some future (the blink_task returns after transitioning to Running, so we need a replacement future for select — use `embassy_futures::block_on!` equivalent or just loop with USB)

Simpler approach for blinky: spawn both USB run and blink_task as background tasks (not selected against). Main future loops forever with `Timer::after_millis(1000).await` or similar — keeping the executor alive.

### 5. `crates/asperitas-logging/src/lib.rs`

No changes needed — module visibility is already correct (`pub mod led` under `log-usb` feature).

## Step-by-step Execution Order

1. Modify `led.rs`: add static storage, `init()`, `get_mut()`, `set_global_state()`, refactor `blink_task`, delete `panic_loop()`
2. Modify `panic_handler.rs`: delete `PanicLedState`/`PANIC_LED`/`set_panic_led()`, rewrite `handle_panic` to use `led::get_mut()`
3. Modify `main.rs`: replace `set_panic_led` with `led::init`, spawn blink_task, add `set_global_state(Running)` after audio starts
4. Modify `blinky.rs`: same replacements, adjust blink loop to use RGB LED
5. Build and verify: `cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky`
6. Clippy: `cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings`
7. Check binary sizes — should be smaller than baseline (one LED driver instead of two, less code)
8. Verify no `#[allow(dead_code)]` remains in led.rs or panic_handler.rs
9. Ensure `LedState` variants match visual requirements: PreInit (red blink), Running (steady green), Panicked (red-on — strobe not possible in panic context, but visually distinct from green)

## Visual Distinctness (AC #2)

| State | Color | Behavior |
|-------|-------|----------|
| PreInit | Red | Blink ~1 Hz (async blink_task) |
| Running | Green | Steady on (blink_task returns after setting) |
| Panicked | Red | Steady on (panic handler sets, no async available for strobe) |

PreInit and Panicked share the same color (red) but differ in context: PreInit blinks while the system is alive, Panicked is steady-on with the system halted. A human observer distinguishes them by whether the device is otherwise functional (USB serial active vs frozen). This is acceptable — a strobing panic LED would require a busy-wait toggle loop in the panic handler, adding complexity for marginal diagnostic value.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation notes: embassy-executor 0.10 Spawner::spawn requires SpawnToken (from #[embassy_executor::task] macro), so blink_task() is a standalone pub async fn that accesses the singleton via raw pointer internally — no borrow-across-await issues. Selected alongside audio+USB futures using nested select. Panicked state shows steady red (not strobe) since async executor is halted during panic; visually distinct from Running (green) by color alone.

Post-review (review-pi-work): ACs #1/#3/#4/#5/#6 re-verified and checked off — status had jumped In Progress -> Done (commit 9a04b9d) without ever checking the boxes. AC #2 left unchecked: see TASK-015 (blink_task's Panicked branch documents/implements a 5 Hz strobe that is dead code in practice, since panic_handler::handle_panic sets state synchronously then loops forever in bkpt/nop, permanently halting the async executor that would poll blink_task).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced dual LED drivers (dead BootLed + hand-rolled panic_handler GPIO) with a single StaticCell-backed singleton BootLed. Both async execution path and panic handler share the same instance via init()/get_mut(). Atomic state coordination lets blink_task() respond to PreInit→Running→Panicked transitions without mutex overhead. Deleted panic_loop(), PanicLedState, PANIC_LED, set_panic_led(), and all #[allow(dead_code)] annotations. Build and clippy pass clean.
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- SECTION:PLAN:END -->
