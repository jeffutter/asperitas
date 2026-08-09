---
id: TASK-028
title: 'Fix: Ticker overrun guard is duplicated and untested at the call site'
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-08-09 05:43'
updated_date: '2026-08-09 06:18'
labels:
  - review-followup
  - planned
dependencies:
  - TASK-026
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-026 (firmware/src/bin/main.rs:139-153, firmware/src/bin/podtest.rs:236-246). The Ticker catch-up overrun guard added by TASK-026 (detect a >2x-period gap, call Ticker::reset(), then always await ticker.next()) is duplicated near-verbatim across main.rs's knob_poll_task and podtest.rs's poll loop — the same policy decision (the 2x-period threshold, the reset-then-await sequencing) living in two places (Organized axis: information leakage — a future change to the threshold or the reset semantics has to be made twice and can silently drift). Separately, TASK-026's AC #2 asked for a test demonstrating the call-site guard prevents a burst of near-zero-elapsed-time polls after a stall; the test that shipped (crates/asperitas-pod/src/encoder.rs, burst_of_identical_readings_emits_edge_without_wall_clock) instead asserts the *opposite* — that DebouncedSwitch::update() DOES emit a spurious edge on a compressed burst — and never exercises the overrun-detection/reset code at all, since that logic lives inline in the two firmware binaries, not in the host-testable asperitas-pod crate. So the actual guard behavior (does a stall really produce at most one immediate tick, not zero, not two?) has no test coverage — a regression like the one just fixed as a post-review fixup (074b49d, folded into b8be5ff) would ship silently again.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The overrun-detection predicate (given now, last_tick, and the poll period, should the ticker be reset?) is extracted into a single host-testable function in crates/asperitas-pod (e.g. a free function taking embassy_time::Instant/Duration), and both firmware/src/bin/main.rs and firmware/src/bin/podtest.rs call it instead of duplicating the threshold check inline
- [ ] #2 A new unit test in crates/asperitas-pod asserts the extracted predicate returns true only when the gap exceeds 2x the poll period, false otherwise (covering the boundary at exactly 2x)
- [ ] #3 A new unit test (in crates/asperitas-pod or as an integration-style test callable without hardware) proves that, given a simulated stall followed by the guard's reset-then-await sequencing, at most one immediate (near-zero-wait) poll iteration occurs — not two, matching the module docs' 'at most one immediate tick' claim
- [ ] #4 nix develop -c cargo test -p asperitas-pod passes
- [ ] #5 nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main --release succeeds
- [ ] #6 nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Extract the duplicated ticker overrun guard into a shared, host-testable function in crates/asperitas-pod, then replace both firmware call sites and add boundary-condition + simulation tests.

## Step 1: Add embassy-time dependency to asperitas-pod

Add  to  . The types (, ) are plain  structs backed by u64 — usable without hardware. For host-side tests, construct values arithmetically from  + ; no time driver needed because the predicate function does not call .

## Step 2: Create  module

New file :

embassy_time::Ticker::next()Ticker::reset()Ticker::reset()Ticker::next()

Add  to  (always available, no feature gate — pure arithmetic on Copy types).

## Step 3: Update  knob_poll_task

Replace the inline guard block (~lines 139-153) with:

Note:  stays before  (the fixup from commit 074b49d). Only the boolean predicate moves out; the sequencing semantics stay local.

## Step 4: Update  poll loop

Apply the identical transformation to the inline guard block (~lines 236-246):

## Step 5: Add boundary-condition unit test (AC #2)

In , add  with a table test:

| gap_ms | period_ms | expected | reason |
|--------|-----------|----------|--------|
| 1      | 1         | false    | gap == period (normal) |
| 2      | 1         | false    | gap == 2*period (boundary, strictly greater-than) |
| 3      | 1         | true     | gap > 2*period (overrun detected) |
| 1000   | 1         | true     | large stall |
| 1      | 500       | false    | sub-threshold relative to long period |

Construct Instants as  and . No time driver needed — pure arithmetic on u64-backed types.

## Step 6: Add simulation test proving at-most-one-immediate-tick (AC #3)

Write a host-side simulation that mimics the poll loop structure using fake timestamps:

The key assertion: after injecting a stall timestamp, the number of subsequent iterations with gap ≤ period is exactly 1 (not 0, not 2+). This proves the  fixup prevents the cascade described in TASK-026's commit 074b49d.

## Step 7: Update encoder.rs docs

Move the overrun-guard explanation from the module-level doc comment and  doc into 's module docs. In , shorten the references to redirect comments like:

> Callers must bound Ticker catch-up bursts using  (see that module for policy details).

This avoids duplicating the explanation in three places.

## Step 8: Verification

Run:
- sync hooks: ✔️(pre-push, pre-commit)

running 25 tests
test encoder::tests::all_lut_entries_valid ... ok
test encoder::tests::bounce_both_bits_00_to_11_yields_zero ... ok
test encoder::tests::bounce_both_bits_01_to_10_yields_zero ... ok
test encoder::tests::burst_of_identical_readings_emits_edge_without_wall_clock ... ok
test encoder::tests::counter_clockwise_one_detent_from_zero ... ok
test encoder::tests::clockwise_one_detent_from_zero ... ok
test encoder::tests::full_clockwise_cycle ... ok
test encoder::tests::full_counter_clockwise_cycle ... ok
test encoder::tests::lut_symmetry_clockwise_vs_counter ... ok
test encoder::tests::long_held_stable_reading_does_not_overflow_consecutive_counter ... ok
test encoder::tests::mixed_rotation_and_bounce_filters_bounce ... ok
test encoder::tests::no_movement_yields_zero ... ok
test encoder::tests::out_of_range_state_is_masked_not_indexed_out_of_bounds ... ok
test encoder::tests::rapid_bounce_sequence_no_false_edges ... ok
test encoder::tests::press_then_release_then_repress_no_double_count ... ok
test encoder::tests::release_after_stable_period_emits_release ... ok
test encoder::tests::single_bounce_pulse_does_not_emit_edge ... ok
test encoder::tests::stable_press_emits_single_edge ... ok
test encoder::tests::stable_state_never_emits_edge ... ok
test knob::tests::normalize_full_range ... ok
test knob::tests::normalize_midpoint ... ok
test knob::tests::normalize_zero ... ok
test knob::tests::spans_the_full_unit_interval ... ok
test knob::tests::twelve_bit_full_scale_is_not_full_scale ... ok
test knob::tests::monotonic_increase ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s — all tests pass including new ones
- sync hooks: ✔️(pre-commit, pre-push) — firmware builds
- sync hooks: ✔️(pre-commit, pre-push) — no warnings
<!-- SECTION:PLAN:END -->
