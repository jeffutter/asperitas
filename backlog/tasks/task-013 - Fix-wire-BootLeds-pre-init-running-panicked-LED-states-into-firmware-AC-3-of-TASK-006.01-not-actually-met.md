---
id: TASK-013
title: >-
  Fix: wire BootLed's pre-init/running/panicked LED states into firmware (AC #3
  of TASK-006.01 not actually met)
status: To Do
assignee: []
created_date: '2026-08-01 21:55'
updated_date: '2026-08-01 21:55'
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
- [ ] #1 firmware/src/bin/main.rs and blinky.rs each drive the RGB LED (PC1/PA6/PA7) through a single owner that exposes both boot-stage states (PreInit before audio/blink starts, Running once it does) and the panicked state, resolving the Peri ownership conflict noted in TASK-006.01.05's implementation notes (e.g. panic_handler owns/borrows the same BootLed instance main.rs holds, rather than a second raw-GPIO copy)
- [ ] #2 the three states are visually distinct on real RGB hardware semantics (not just on/off): pre-init and running are distinguishable from each other and from panicked, consistent with led.rs's documented LedState variants
- [ ] #3 panic_handler.rs's LED write reuses led.rs's color-setting logic (BootLed or the shared LED_ACTIVE_LOW-driven helper) instead of a second hand-rolled raw-GPIO Output implementation
- [ ] #4 led.rs has no dead code left un-integrated: delete #[allow(dead_code)] on panic_loop once it is actually reachable, or delete panic_loop/blink_task if the chosen design does not need them — no #[allow(dead_code)] should remain masking unused public API
- [ ] #5 nix develop -c cargo build --release --features seed3 --bin main --bin blinky (from firmware/) succeeds
- [ ] #6 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust firmware project (firmware/, Embassy on Daisy Seed3) plus a host-side Rust workspace (crates/asperitas-dsp, crates/asperitas-cli, crates/asperitas-logging). ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions (daisy-embassy's rev, embassy-* versions, etc.).

## Background

crates/asperitas-logging/src/led.rs defines BootLed with LedState::{PreInit,Running,Panicked}, a blink_task(), and a panic_loop() — a complete, well-designed LED boot-stage indicator. None of it is wired into firmware/src/bin/main.rs or blinky.rs. TASK-006.01.05's implementation notes explain why: BootLed::new() and panic_handler::set_panic_led() both need to own the same three RGB Peri tokens (PC1/PA6/PA7, i.e. board.pins.d20/d19/d18), and rather than resolve that single-ownership conflict, the integration ticket called set_panic_led() only and used the unrelated single-color onboard user_led (PC7) for a bare on()/off() boot indicator. BootLed is therefore dead code, and panic_handler.rs::handle_panic hand-rolls its own raw hal::gpio::Output red-on/green-off/blue-off logic instead of calling into BootLed — duplicating led.rs's polarity-handling logic (the LED_ACTIVE_LOW branches) in a second place.

## Steps

1. Read crates/asperitas-logging/src/led.rs in full (BootLed, LedState, set_state, blink_task, panic_loop) and crates/asperitas-logging/src/panic_handler.rs in full (set_panic_led, handle_panic) to understand the current split.
2. Resolve the ownership conflict by giving panic_handler.rs a way to reach into a BootLed that main.rs/blinky.rs already own, instead of panic_handler.rs owning a second raw copy of the same three pins. The simplest shape: have set_panic_led take (or construct) a BootLed and store *that* in the panic-handler's static, then have handle_panic call led.set_state(LedState::Panicked) (or a variant of panic_loop's logic) through it, and have main.rs/blinky.rs keep their own handle/reference for PreInit/Running via the same static (behind a critical section, matching the existing PANIC_LED pattern) — or, if that creates awkward aliasing, make BootLed itself own both the boot-stage and panic responsibilities and have main.rs call a single asperitas_logging::led::init(...) that returns something both main() and the panic handler can reach (mirroring how usb.rs's init()/run() split already works). Pick whichever shape keeps a single owner of the three Peri tokens — this is a real design decision, not mechanical, so use judgement and record the choice in the task's implementation notes.
3. Update firmware/src/bin/main.rs: call the new boot-stage API to set PreInit right after board init (before audio prepare) and Running right before entering the audio callback loop, replacing the current bare board.user_led.on() call.
4. Update firmware/src/bin/blinky.rs similarly: PreInit before the blink loop starts, Running (or just leave blinking as today, but ensure PreInit is visible briefly at boot) — use judgement matching main.rs's approach for consistency.
5. Update panic_handler.rs::handle_panic to drive the LED through the same BootLed-backed path rather than a second raw hal::gpio::Output. Remove the now-redundant PanicLedState struct/PANIC_LED static if BootLed's own static replaces it, or fold them together — avoid having two parallel 'set of 3 GPIO outputs plus polarity logic' implementations.
6. Once panic_loop and blink_task are genuinely reachable from the binaries, remove the #[allow(dead_code)] on panic_loop (or delete panic_loop/blink_task if the chosen design in step 2 doesn't end up using them — do not leave unused pub API with a dead_code lint silenced).
7. Verify binary sizes still fit within the 128 KB internal flash limit (check with cargo build --release, look at the size the linker reports or use cargo-binutils size if available — TASK-006.01.05 recorded main ~65.5 KB, blinky ~49.8 KB as the baseline).
8. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky
9. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings
10. Update crates/asperitas-logging/src/led.rs doc comments if the Panicked blink rate or behavior changed from the original ~5 Hz strobe design during integration.
11. Check off TASK-006.01's AC #3 (crates/asperitas-logging is a sibling file, not this task, so this is a note for the implementer: leave TASK-006.01 alone, its AC #3 was intentionally left unchecked pending this fix — do not check it as part of this ticket; a human or the next review pass will do that once TASK-006.02 confirms it on hardware).
<!-- SECTION:PLAN:END -->
