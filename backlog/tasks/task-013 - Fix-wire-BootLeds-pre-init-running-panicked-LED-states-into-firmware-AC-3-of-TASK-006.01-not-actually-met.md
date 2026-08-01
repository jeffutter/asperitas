---
id: TASK-013
title: >-
  Fix: wire BootLed's pre-init/running/panicked LED states into firmware (AC #3
  of TASK-006.01 not actually met)
status: To Do
assignee: []
created_date: '2026-08-01 21:55'
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
SETUP (read first): This is a Rust+WebAssembly... wait
<!-- SECTION:PLAN:END -->
