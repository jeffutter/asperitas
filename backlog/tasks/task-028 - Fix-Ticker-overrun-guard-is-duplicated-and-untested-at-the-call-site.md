---
id: TASK-028
title: 'Fix: Ticker overrun guard is duplicated and untested at the call site'
status: To Do
assignee: []
created_date: '2026-08-09 05:43'
labels:
  - review-followup
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
SETUP (read first): This is a Rust+WebAssembly core (crates/gql-core) with a
TypeScript/React web app (web/). ALL commands must run inside the Nix dev
shell: either run 'direnv allow' once, or prefix every command with
'nix develop -c'. Work from the repository root unless told otherwise. Do not
change pinned dependency versions.

Note: this specific project has no crates/gql-core or web/ — ignore those paths in the preamble above; they are boilerplate from the review skill. The actual relevant crate is crates/asperitas-pod, and the firmware binaries are firmware/src/bin/main.rs and firmware/src/bin/podtest.rs.

1. In crates/asperitas-pod/src/encoder.rs (or a new small module, e.g. crates/asperitas-pod/src/ticker_guard.rs, whichever fits the existing module layout better after reading lib.rs), add a pure function such as:
   pub fn should_reset_on_overrun(now: embassy_time::Instant, last_tick: embassy_time::Instant, period: embassy_time::Duration) -> bool {
       now - last_tick > period * 2
   }
   Check whether embassy_time::Instant/Duration are usable outside the hw-gated cfg in this crate today (they likely already are, since firmware/src/bin/main.rs and podtest.rs use them and depend on asperitas-pod) — if embassy_time is only a dev-dependency or feature-gated in asperitas-pod's Cargo.toml, add it as a normal dependency first and confirm it builds host-side (Instant::now() needs a time driver; for host tests you may need embassy_time's std feature — check Cargo.toml under [dev-dependencies] and asperitas-pod/Cargo.toml for existing embassy_time wiring before assuming a new feature is needed).
2. Replace the inline gap-check-and-reset block in firmware/src/bin/main.rs's knob_poll_task (currently around lines 139-153, post-fixup) with a call to the new shared predicate: keep the reset()/next().await sequencing local to the loop (Ticker isn't Send/shareable across a free function boundary in a way that's worth abstracting — only the boolean decision should move), i.e.:
   let now = embassy_time::Instant::now();
   if asperitas_pod::should_reset_on_overrun(now, last_tick, embassy_time::Duration::from_millis(POLL_INTERVAL_MS)) {
       ticker.reset();
   }
   ticker.next().await;
   last_tick = now;
3. Apply the identical change to firmware/src/bin/podtest.rs's poll loop (currently around lines 236-246, post-fixup).
4. Add the boundary-condition unit test described in AC #2 to crates/asperitas-pod (table-test a few (gap, expected) pairs: gap == period (false), gap == 2*period (false, boundary is strictly greater-than per the existing behavior), gap == 2*period + 1 tick / 1ms (true)).
5. For AC #3 (proving the sequencing bounds catch-up to one immediate tick, not two): this cannot use a real embassy executor without hardware, so instead write a small host-side simulation: a loop that mimics the poll task's structure using a fake/injected 'now' sequence (a Vec<Instant> or a simple counter-based clock) driving should_reset_on_overrun and counting how many 'body' executions happen with near-zero (< 1 poll period) gaps between them after a simulated large gap is injected once. Assert the count of near-zero-gap body executions immediately following the injected stall is exactly 1, not 2 — this is the regression the post-review fixup (see TASK-026's Implementation Notes, commit 074b49d/fixup!b8be5ff) corrected by hand; this test is what should have caught it.
6. Update crates/asperitas-pod/src/encoder.rs's module doc / ControlSurface::poll() doc comment if the extraction moves the overrun-bound explanation out of the call sites and into the new shared function's doc comment — keep the two docs consistent, don't duplicate the explanation in three places now.
7. Run: nix develop -c cargo test -p asperitas-pod
8. Run: nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main --release
9. Run: nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --bin main -- -D warnings
<!-- SECTION:PLAN:END -->
