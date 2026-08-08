---
id: TASK-021
title: >-
  Fix: podtest polls ControlSurface at 100 Hz, 10x under its documented ~1 kHz
  contract
status: Dev Ready
assignee: []
created_date: '2026-08-08 05:07'
updated_date: '2026-08-08 05:16'
labels:
  - review-followup
  - planned
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
## Approach: Throttle knob logging at 1 kHz poll rate

Raise the main loop to ~1 kHz to satisfy ControlSurface's documented contract, but throttle knob-value logging to ~100 Hz so USB CDC serial remains readable. This is option (c) from the ticket description — correct polling + readable output with minimal structural complexity.

### Rationale

- The Gray-code LUT in encoder.rs requires ≥1 kHz polling to avoid missing detent transitions. At 100 Hz, brisk turns can produce two state changes within one 10 ms window, causing silent detent drops.
- Knob logging at 1 kHz (~18 bytes/tick × 1000 = ~18 KB/s) would flood the terminal. Throttling to every 10th tick keeps output at the current ~100 Hz rate.
- Alternative (split into two async tasks) adds unnecessary complexity for a diagnostic binary. A single throttled loop is simpler and has no scheduling drift concerns.

### Steps

1. **Change POLL_INTERVAL_MS** from 10 → 1 (raises loop to ~1 kHz).
2. **Change LED_TICKS_PER_COLOR** from 100 → 1000 (keeps ~1 second LED cadence at the new rate).
3. **Add knob logging throttle** — introduce a  counter that increments each iteration and only calls  for knob values when . Reset to 0 after wrapping. This keeps knob output at ~100 Hz while controls.poll() runs at ~1 kHz.
4. **Update doc comments** — explain why POLL_INTERVAL_MS = 1 (encoder contract reference) and document the throttle factor.
5. **Build & lint** — cargo build --release + clippy with -D warnings.
<!-- SECTION:PLAN:END -->
