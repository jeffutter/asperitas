---
id: TASK-015
title: >-
  Fix: led::blink_task's Panicked branch documents a 5 Hz strobe that is dead
  code, contradicting LedState's own doc
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-01 22:30'
updated_date: '2026-08-01 22:45'
labels:
  - review-followup
dependencies:
  - TASK-013
priority: high
type: bug
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-013 (crates/asperitas-logging/src/led.rs, async fn blink_task, LedState::Panicked branch: 'Red blink ~5 Hz' via set_color_on/Timer::after_millis(100) loop). This branch can never execute in production: panic_handler::handle_panic (crates/asperitas-logging/src/panic_handler.rs) sets LED state synchronously via led::set_global_state(Panicked) — which calls BootLed::set_state once, a steady GPIO write, not a loop — and then enters bkpt() followed by an infinite nop() loop, per its own comment 'During panic, the async executor is halted'. Once handle_panic runs, blink_task (an async task on the same single-core cooperative executor) is never polled again, so its Panicked match arm's 5 Hz strobe logic is unreachable at runtime. This directly contradicts LedState::Panicked's own doc comment ('Red on — unrecoverable error / panic', i.e. steady) and TASK-013's own Implementation Notes ('Panicked state shows steady red (not strobe) since async executor is halted during panic'). It is exactly the doc-vs-reality mismatch TASK-013 was created to eliminate in TASK-006.01 (original problem: led.rs documented 'Fast strobe red (~5Hz)' for Panicked while the shipped behavior was steady-on) — the same contradiction has been reintroduced, just moved into blink_task's per-state comment and match arm instead of the enum doc. Correct/Clarity axis: a maintainer reading blink_task's doc or match arm will believe a real panic produces a 5 Hz strobe; it does not, and the code implementing that belief is unreachable in the one context it exists for.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 led.rs's blink_task has no code path that implements behavior it cannot actually reach at runtime — either delete the unreachable Panicked match arm's strobe logic (collapse it to match set_state's steady-on, or simply remove the Panicked arm from blink_task's loop since panic_handler already owns that transition exclusively) or make it reachable (e.g. panic_handler drives an explicit busy-wait strobe loop itself instead of relying on the halted async executor)\n#2 blink_task's doc comment and LedState::Panicked's doc comment agree with each other and with the code that actually runs during a real panic — no comment claims a 5 Hz strobe unless the code path that produces it is demonstrably reachable\n#3 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky succeeds\n#4 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust firmware project (firmware/, Embassy on Daisy Seed3) plus a host-side Rust workspace (crates/asperitas-dsp, crates/asperitas-cli, crates/asperitas-logging). ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Read crates/asperitas-logging/src/led.rs in full, specifically: the LedState enum doc comments (top of file), BootLed::set_state (steady, synchronous — used by both set_global_state and, transitively, the panic handler), and async fn blink_task's LedState::Panicked match arm (the ~5 Hz Timer::after_millis(100) loop).
2. Read crates/asperitas-logging/src/panic_handler.rs::handle_panic and confirm: it calls crate::led::set_global_state(LedState::Panicked) once (synchronous steady write), then cortex_m::asm::bkpt() followed by an infinite nop() loop — it never returns, so the async executor that polls blink_task is never scheduled again after a real panic.
3. Decide and implement one of the two resolutions (prefer the first — it matches the Implementation Notes' own stated rationale that a strobing panic LED is not worth the added complexity):
   a. Simplify: remove the Panicked match arm from blink_task's loop body (panic_handler already sets the LED to steady red directly via set_state before halting; blink_task never needs to render Panicked itself since it is never polled after a real panic). Update blink_task's doc comment to say only PreInit/Running behavior, and note that Panicked is set once, synchronously, by the panic handler and is never rendered by this task.
   b. OR make it real: have handle_panic perform an explicit synchronous busy-wait strobe (toggle BootLed::set_color_on in a bkpt-free spin loop with a cycle-count delay) instead of relying on blink_task. Only choose this if a real strobe is judged worth the added panic-handler complexity — if so, delete the now-redundant Panicked arm from blink_task entirely (it would be fully superseded) and document the busy-wait invariant.
4. Update LedState::Panicked's doc comment and blink_task's own doc comment so both describe the same, actually-reachable behavior — no leftover claim of a 5 Hz strobe unless (b) was chosen and actually implements one.
5. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky
6. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings
7. Re-read crates/asperitas-logging/src/led.rs top-to-bottom once more and confirm every doc comment on LedState and blink_task matches the code that runs it — this ticket exists because that invariant broke once already.
<!-- SECTION:PLAN:END -->
