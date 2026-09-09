---
id: TASK-040
title: >-
  Fix: write_whole's sink-stall path hangs silently in release builds instead of
  failing loud
status: To Do
assignee: []
created_date: '2026-09-09 16:46'
labels:
  - review-followup
dependencies:
  - TASK-030.01.01
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-030.01.01 (crates/asperitas-logging/src/frame.rs:306-328, write_whole). When the caller-supplied write() closure stalls (returns None or Some(0)) after the initial capacity pre-check already passed, the loop hits debug_assert!(false, ...) and falls through to retry the identical write() call with no bound. debug_assert! compiles to a no-op in release builds — the profile that actually ships to the Seed3 — so a precondition violation there becomes a silent, unbounded busy-loop with zero diagnostic output, plausibly inside the caller's lock or a critical section, hanging the whole device. This defeats the project's existing fail-loud infrastructure (TASK-006's custom panic handler drives an LED strobe and a USB serial message on panic) — the stall path bypasses it entirely instead of triggering it. Violates the Resilient axis: an unreachable-by-precondition state should fail loud, not degrade to an undiagnosable hang. Currently unreachable given today's call sites and embassy-sync version, so this is latent rather than actively triggered, but it is a real footgun if the precondition (single locked writer, capacity already checked) ever quietly stops holding — e.g. an embassy-sync version bump changing free_capacity()/try_write() short-write behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 write_whole's stall branch (frame.rs, the match arm currently reading debug_assert!(false, ...)) panics unconditionally in both debug and release profiles when write() returns None or Some(0) after the length pre-check already passed — replace the debug_assert! with a real panic! (or equivalent unconditional assert), so the project's panic handler (LED strobe + USB serial message) fires instead of an infinite retry.
- [ ] #2 A new #[cfg(test)] unit test in frame.rs constructs a write closure that accepts some bytes on its first call and then returns Some(0) on a later call (simulating the sink stalling after the capacity pre-check passed), and asserts write_whole panics for it (#[should_panic] or equivalent), proving the precondition-violation path is now reachable and tested rather than merely assumed unreachable.
- [ ] #3 write_whole's doc comment states plainly that a stall after the capacity pre-check is treated as a caller-contract violation and panics immediately (via the project's normal panic handler) rather than retrying, replacing the current comment's silence on release-build behavior.
- [ ] #4 nix develop -c cargo fmt --all --check passes
- [ ] #5 nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings passes
- [ ] #6 nix develop -c cargo test -p asperitas-logging passes, including the new should-panic test
- [ ] #7 nix develop -c cargo test --workspace passes and cd firmware && nix develop -c cargo build --release --features seed3 succeeds, with firmware/Cargo.lock showing no diff
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust embedded-audio project (crates/*, firmware/) targeting the Daisy Seed3 in a Daisy Pod. ALL commands must run inside the Nix dev shell: prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Open crates/asperitas-logging/src/frame.rs and locate pub fn write_whole (currently around line 306). Its loop body has a match arm:
   stalled => {
       debug_assert!(
           false,
           "sink stalled after the capacity pre-check passed: {stalled:?}"
       );
   }
   This arm falls through to the top of the while loop and retries write() on the identical unconsumed slice with 'written' unchanged. In a release build debug_assert! is a no-op, so this becomes an unbounded busy-loop with no forward progress and no diagnostic — the caller's lock or critical section (if any) is held the whole time, and none of the project's panic-handler diagnostics (LED strobe, USB serial message) ever fire.

2. Replace the debug_assert! with an unconditional panic! that fires in every build profile:
   stalled => {
       panic!("write_whole: sink stalled after the capacity pre-check passed ({stalled:?}); this violates write_whole's precondition that the caller holds the lock and already verified free_capacity, so continuing would either hang forever or silently commit a partial frame");
   }
   This is a deliberate departure from the original plan's 'let release builds finish the loop' — that phrasing assumed the loop would eventually make progress, but the code as written cannot: nothing changes about the sink or the slice between retries, so a genuine stall is unbounded, not transient. A real panic here restores the project's normal fail-loud path (the custom panic handler from TASK-006 strobes an LED and emits a USB serial message on panic) instead of an undiagnosable hang. Keep the interpolated stalled value in the message so a panic report (if captured) names the actual return value that violated the contract.

3. Update write_whole's doc comment (directly above the function) to state this behavior plainly: a sink stall after the capacity pre-check has already passed is a caller-contract violation (the caller must hold the lock and have already confirmed frame.len() <= free_capacity for the whole call), and such a violation panics immediately via the project's normal panic handler rather than retrying silently. Remove or correct any wording that implied release builds would 'finish the loop' — they no longer do, by design.

4. Add a new test in the existing '#[cfg(test)] mod tests' block in frame.rs (follow the file's existing test style: descriptive snake_case name, a rationale comment above anything non-obvious). Name it something like write_whole_panics_when_the_sink_stalls_after_the_precheck. Use a closure with interior mutability (e.g. a std::cell::Cell<u32> call counter, or a small local struct implementing FnMut via a closure capturing a mutable local) that:
   - On its first invocation, accepts part of the frame (returns Some(n) for some n < frame.len()).
   - On its second invocation, returns Some(0) (or None) to simulate the stall.
   Call write_whole with a frame and free_capacity large enough to pass the initial pre-check, and assert the call panics — use #[test] #[should_panic] with an expect substring matching part of the new panic message (e.g. expected = "sink stalled"), or catch_unwind if the surrounding test module already uses that idiom elsewhere (check first; match existing convention rather than introducing a new one).

5. Run the verification gates in order and fix anything that regresses:
   nix develop -c cargo fmt --all --check
   nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings
   nix develop -c cargo test -p asperitas-logging
   nix develop -c cargo test --workspace
   cd firmware && nix develop -c cargo build --release --features seed3
   git diff --stat firmware/Cargo.lock   # must be empty — this ticket touches no dependencies

6. In the Final Summary, state explicitly that this changes write_whole's behavior on an already-documented-as-unreachable precondition violation from 'silent infinite retry in release' to 'unconditional panic in every profile', and why that is the correct fail-loud choice given the project's existing panic-handler diagnostics.
<!-- SECTION:PLAN:END -->
