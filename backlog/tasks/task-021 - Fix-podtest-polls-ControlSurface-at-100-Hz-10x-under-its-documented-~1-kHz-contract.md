---
id: TASK-021
title: >-
  Fix: podtest polls ControlSurface at 100 Hz, 10x under its documented ~1 kHz
  contract
status: To Do
assignee: []
created_date: '2026-08-08 05:07'
labels:
  - review-followup
dependencies:
  - TASK-020
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-020 (firmware/src/bin/podtest.rs:120-164, crates/asperitas-pod/src/encoder.rs:19-20,212). ControlSurface::poll()'s own doc comment says 'Call from a control-surface task at ~1 kHz' and the module doc explains why: DEBOUNCE_TICKS=5 is calibrated for ~5 ms debounce at 1 kHz, and the quadrature Gray-code LUT in EncoderDecoder::update() assumes each poll sees at most one A/B transition — skip a transition (state jumps e.g. 00 to 11 between polls) and the LUT reads it as a bounce (delta=0), silently dropping detents. podtest.rs's main loop polls knobs.read() and controls.poll() together at 10 ms intervals (POLL_INTERVAL_MS=10, i.e. 100 Hz) — 10x slower than the documented contract. Button debounce still works but balloons to ~50 ms (cosmetic). The encoder is the real risk: on a brisk detent turn, two Gray-code transitions can land inside one 10 ms window, so podtest can undercount detents through no fault of the underlying TASK-018.03 driver. This directly undermines TASK-018.04 AC #3 ('the encoder produces one increment per physical detent in both directions') — a human tester could see missed/inconsistent counts and wrongly conclude ControlSurface itself is broken. Correctness axis: podtest.rs violates a documented precondition of the function it calls. Needs a decision, not a blind bump: either raise podtest's loop to ~1 kHz (verify knob logging volume over USB CDC is still tractable at that rate) or split encoder/button polling onto its own faster interval from knob-value logging.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 firmware/src/bin/podtest.rs polls ControlSurface at a rate consistent with crates/asperitas-pod/src/encoder.rs's documented ~1 kHz contract (either by raising the shared loop rate or by polling controls on a separate faster interval), with the choice and rationale recorded in the task's implementation notes
- [ ] #2 knob value logging over USB CDC remains readable/parseable at whatever final rate is chosen (no line-rate flooding that breaks the terminal use case described in TASK-020)
- [ ] #3 nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release succeeds
- [ ] #4 nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust embedded firmware project (firmware/, crates/) for the Daisy Seed3 in a Daisy Pod, built with embassy-stm32/embassy-executor async on no_std. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Read crates/asperitas-pod/src/encoder.rs lines 1-31 and the ControlSurface::poll() doc comment (~line 263-270) to confirm the ~1 kHz contract and why DEBOUNCE_TICKS=5 / the Gray-code LUT assume it.
2. Read firmware/src/bin/podtest.rs's main loop (poll_fut, lines ~120-164). Decide between two approaches:
   a. Raise POLL_INTERVAL_MS from 10 to 1 (both knobs.read() and controls.poll() run at ~1 kHz). Check whether logging every knob reading at 1 kHz floods the USB CDC serial line unreadably — if so, keep knob logging throttled (e.g. log every Nth read, still read/react every tick) while polling both at 1 kHz.
   b. Split into two loop rates: poll controls.poll() (encoder/buttons) at ~1 kHz for correctness, but only read/log knobs.read() and check the LED tick counter every 10th iteration (~100 Hz), so knob logging volume is unchanged.
   Prefer (b) if (a) makes the serial log unusable — pick whichever keeps the diagnostic tool actually readable by a human while meeting ControlSurface's documented rate.
3. Update LED_TICKS_PER_COLOR and any tick-based counters to keep the ~1 second LED cadence correct under the new loop rate.
4. Update the doc comment above POLL_INTERVAL_MS (and add one above the new constant if you introduce a second interval) explaining why the rate was chosen, referencing encoder.rs's documented contract.
5. Run: nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release
6. Run: nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest -- -D warnings
7. Run: nix develop -c cargo fmt --manifest-path firmware/Cargo.toml --check
8. Record the chosen approach and rationale in the task's implementation notes so TASK-018.04's human tester knows what rate to expect the diagnostic output at.
<!-- SECTION:PLAN:END -->
