---
id: TASK-045
title: >-
  Fix: write_whole's caller-contract panic can permanently mask interrupts,
  silencing the panic text it exists to deliver
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 07:02'
labels:
  - review-followup
dependencies:
  - TASK-040
priority: high
type: bug
ordinal: 110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-040 (crates/asperitas-logging/src/frame.rs write_whole, and its two call sites crates/asperitas-logging/src/lib.rs:329 emit() and lib.rs:465 try_emit_dump()). TASK-040 made write_whole panic unconditionally (every build profile) on a sink stall, and both call sites invoke write_whole from inside RECORD_BUFS.lock(), a CriticalSectionRawMutex whose lock() masks all interrupts (PRIMASK) for its duration via embassy_sync -> critical_section::with. This target's panic strategy is abort (verified: rustc --print cfg --target thumbv7em-none-eabihf reports panic="abort"), so a panic inside that critical section does not unwind and never runs critical_section::with's Guard::drop -- the only thing that calls critical_section::release() to restore PRIMASK. A stall-triggered panic therefore leaves interrupts globally masked for the remaining lifetime of the program. panic_handler::handle_panic() then calls usb::emit_panic_record -> usb::emit_blocking, whose own doc comment says this works 'because the USB interrupt handler is still installed and still firing during the panic spin' -- an assumption this exact scenario breaks, so emit_blocking spins for its full EMIT_TIMEOUT with no USB progress and the panic text is silently dropped; only the LED reaches the user. This directly contradicts TASK-040's own stated goal (panicking 'routes the failure through the project's existing fail-loud infrastructure ... emits the panic text ... over USB serial') for exactly the path it added, and usb::emit_panic_record's own pre-existing doc comment names the invariant this breaks: 'the one theoretical overlap (a panic raised inside the commit critical section) cannot happen in release, where nothing in that region panics' -- true before TASK-040, false after it. Violates the Resilient axis (CLAUDE.md: fails safe; compiling is not evidence) and the Correct axis (the ticket's claimed behavior does not hold for the path it added). A second, related doc inaccuracy in the same file: lib.rs's try_emit_dump doc block (~line 427) claims 'While the lock is held the consumer still runs -- it runs with interrupts enabled, so free capacity can only grow here.' Since RECORD_BUFS.lock() is the same interrupt-masking critical section, the consumer cannot run at all while the lock is held; the correct (and actually stronger) invariant is that free capacity is frozen, not growing, for the duration. Fix this doc block in the same pass, since fixing the panic path requires rewriting it anyway.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 write_whole (frame.rs) no longer panics itself; it returns a result distinguishing the three outcomes (committed, refused up front for lack of space, sink stalled after partial progress) instead of bool. Its doc comment states plainly that the caller, not write_whole, is responsible for panicking on the stalled outcome, and must do so only after any lock/critical section held during the call has been released.
- [ ] #2 Both call sites in crates/asperitas-logging/src/lib.rs (emit() and try_emit_dump()) propagate the stalled outcome out of their RECORD_BUFS.lock(|cell| {...}) closure and panic (message equivalent to today's write_whole stall message) only after that closure has returned -- i.e. outside the critical section. Neither closure panics internally on this path anymore.
- [ ] #3 The frame.rs unit test exercising the stall path is updated to assert write_whole's returned Stalled outcome directly (no #[should_panic] on write_whole itself), with a short comment explaining that the panic itself now happens in the caller and is therefore only reachable under the log-usb firmware target, not on host.
- [ ] #4 lib.rs's try_emit_dump doc block is corrected: RECORD_BUFS.lock() masks interrupts globally for its duration (CriticalSectionRawMutex), so the consumer cannot run at all while the lock is held and free capacity is frozen (not merely non-decreasing) during the call; the now-false 'write_whole's stall panic cannot fire on this path' claim is replaced with an accurate statement of where that panic fires after this fix.
- [ ] #5 usb.rs's emit_panic_record doc comment ('cannot happen in release, where nothing in that region panics') and emit_blocking's 'USB interrupt handler is still installed and still firing' assumption are re-verified true given this fix (panic no longer fires inside RECORD_BUFS.lock()), and reworded only if the fix's exact mechanics require it to stay accurate.
- [ ] #6 nix develop -c cargo fmt --all --check passes
- [ ] #7 nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings passes
- [ ] #8 nix develop -c cargo test -p asperitas-logging passes, including the updated stall-path test
- [ ] #9 nix develop -c cargo test --workspace passes and cd firmware && nix develop -c cargo build --release --features seed3 succeeds, with firmware/Cargo.lock showing no diff
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust embedded-audio project (crates/*, firmware/) targeting the Daisy Seed3 in a Daisy Pod. ALL commands must run inside the Nix dev shell: prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

## 1. Facts established by this review pass -- do not re-research

- RECORD_BUFS (lib.rs:290-292) is an embassy_sync blocking_mutex::Mutex<CriticalSectionRawMutex, ...>. Its lock() (embassy_sync) calls critical_section::with internally, which on cortex-m masks PRIMASK for the closure's duration and restores it via a Guard whose Drop calls critical_section::release() -- see ~/.cargo/registry/src/*/critical-section-1.2.0/src/lib.rs around fn with().
- Guard::drop() only runs if the closure returns normally (including via early return) or unwinds. This target's panic strategy is abort, confirmed by: nix develop -c rustc --print cfg --target thumbv7em-none-eabihf | grep panic -> panic="abort". Under abort, a panic! inside the closure calls the #[panic_handler] directly with no unwinding, so Guard::drop() (and therefore critical_section::release()) never runs. PRIMASK stays masked forever after such a panic.
- write_whole (frame.rs, currently ~303-341) panics from inside its `write` closure's stall arm. Both call sites (lib.rs emit() at ~322-357, try_emit_dump() at ~461-503) invoke write_whole from inside RECORD_BUFS.lock(|cell| {...}), so a stall panic fires inside the critical section.
- panic_handler::handle_panic() (panic_handler.rs) runs after any panic, unconditionally, and under log-usb calls usb::emit_panic_record -> usb::emit_blocking (usb.rs ~341-381), whose doc comment states it works 'because the USB interrupt handler is still installed and still firing during the panic spin'. If PRIMASK is masked (per above), that is false, and emit_blocking's spin loop will exhaust EMIT_TIMEOUT with no USB progress -- the panic text never reaches the host. Only crate::led::set_global_state (synchronous GPIO, called before the USB emit) reliably reaches the user in this specific scenario.
- usb.rs's emit_panic_record doc comment already states the invariant this breaks: 'the one theoretical overlap (a panic raised inside the commit critical section) cannot happen in release, where nothing in that region panics.' That was true before TASK-040 (debug_assert! was a release no-op) and is false after it.
- lib.rs's try_emit_dump doc block (~409-444) contains: 'While the lock is held the consumer still runs -- it runs with interrupts enabled, so free capacity can only *grow* here.' This is wrong given CriticalSectionRawMutex: the consumer (usb::run()'s drain task) cannot run at all while the lock is held, because it needs the USB interrupt (masked) or the executor (single-threaded, and emit()/try_emit_dump() may themselves run from an interrupt context) to make progress. The code is not buggy -- capacity is frozen, not growing, during the lock, which is an even safer invariant for the pre-check -- but the stated reasoning is wrong and must be corrected.

## 2. The fix -- move the panic outside the critical section

Change write_whole so it never panics; instead it returns an outcome the caller inspects and acts on only after RECORD_BUFS.lock() has returned (i.e. after the critical section has been exited normally, restoring PRIMASK via Guard::drop).

### 2a. crates/asperitas-logging/src/frame.rs

Replace write_whole's `-> bool` return with a small outcome enum. Suggested shape (naming may be adjusted for consistency with the rest of the file's style, but keep three variants with this meaning):

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WriteOutcome {
        /// The whole frame was committed to the sink.
        Committed,
        /// The frame did not fit against the pre-check; nothing was written.
        RefusedForSpace,
        /// The sink stalled after the capacity pre-check passed and after some
        /// progress -- a caller-contract violation. The CALLER must panic on this
        /// variant, and only after releasing any lock/critical section it holds
        /// during the call, so the project's fail-loud panic path (LED + serial)
        /// can run with interrupts still live. See emit()/try_emit_dump() in lib.rs.
        Stalled,
    }

    pub fn write_whole(
        frame: &[u8],
        free_capacity: usize,
        mut write: impl FnMut(&[u8]) -> Option<usize>,
    ) -> WriteOutcome {
        if frame.len() > free_capacity {
            return WriteOutcome::RefusedForSpace;
        }
        let mut written = 0usize;
        while written < frame.len() {
            match write(&frame[written..]) {
                Some(n) if n > 0 => written += n,
                _ => return WriteOutcome::Stalled,
            }
        }
        WriteOutcome::Committed
    }

Rewrite the doc comment section currently titled '# A stall after the pre-check panics, in every profile' to state: write_whole itself never panics; a stall after the pre-check is reported as WriteOutcome::Stalled, and the caller is contractually required to panic on it, but only once it is no longer holding whatever lock/critical section protected the call -- because panicking while interrupts are masked (this target's panic strategy is abort; a panic inside a critical_section::with closure never restores PRIMASK) would leave the board silently deaf on top of dead. Reference emit()/try_emit_dump() as the enforcement points.

### 2b. crates/asperitas-logging/src/lib.rs -- emit()

Around line 322-357. Change the RECORD_BUFS.lock(|cell| {...}) closure so it returns a bool ('stalled') instead of (), matching write_whole's new outcome:

    let stalled = RECORD_BUFS.lock(|cell| {
        // ...unchanged setup (seq, fill, encode)...
        let framed = &bufs.frame[..encoded.len];
        match frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
            LOG_PIPE.try_write(chunk).ok()
        }) {
            frame::WriteOutcome::Committed => {
                console::CONSOLE.record_committed();
                false
            }
            frame::WriteOutcome::RefusedForSpace => {
                console::CONSOLE.record_dropped_for_space(framed.len());
                false
            }
            frame::WriteOutcome::Stalled => true,
        }
    });
    if stalled {
        panic!("emit: write_whole's sink stalled after the capacity pre-check passed; this violates write_whole's precondition that the caller holds the lock and already verified free_capacity");
    }

### 2c. crates/asperitas-logging/src/lib.rs -- try_emit_dump()

Around line 461-503. This closure already returns bool as the function's own return value, with early returns for the body-too-long and dump_fits refusals. Change it to return (bool /* committed */, bool /* stalled */) so the stall case can be distinguished after the lock releases, e.g.:

    let (committed, stalled) = RECORD_BUFS.lock(|cell| {
        // ...unchanged setup through the dump_fits check, each early return becomes (false, false)...
        // ...unchanged seq/encode...
        let framed = &bufs.frame[..encoded.len];
        match frame::write_whole(framed, LOG_PIPE.free_capacity(), |chunk| {
            LOG_PIPE.try_write(chunk).ok()
        }) {
            frame::WriteOutcome::Committed => {
                console::CONSOLE.record_committed();
                (true, false)
            }
            frame::WriteOutcome::RefusedForSpace => {
                debug_assert!(
                    false,
                    "pipe refused a {}-byte frame the headroom rule had already admitted",
                    framed.len(),
                );
                console::CONSOLE.record_dropped_for_space(framed.len());
                (false, false)
            }
            frame::WriteOutcome::Stalled => (false, true),
        }
    });
    if stalled {
        panic!("try_emit_dump: sink stalled after the capacity pre-check passed; this violates write_whole's precondition that the caller holds the lock and already verified free_capacity");
    }
    committed

Keep the existing body.len() > MAX_BODY debug_assert! and its early return exactly as today (unrelated to this fix; do not touch it beyond adjusting its return arity to (false, false)).

## 3. Doc corrections

- lib.rs's try_emit_dump doc block (~409-444): replace 'While the lock is held the consumer still runs -- it runs with interrupts enabled, so free capacity can only *grow* here' with an accurate statement: RECORD_BUFS.lock() is a CriticalSectionRawMutex, so it masks interrupts globally for its duration; the consumer (usb::run()'s drain task) cannot run at all while the lock is held; free capacity is therefore frozen, not merely non-decreasing, for the duration of the call, which is what makes the capacity check below sound. Also correct the trailing clause '...and it is why write_whole's stall panic cannot fire on this path' -- it is no longer true that the panic cannot fire; write_whole no longer panics at all, and the caller (try_emit_dump, per 2c) panics after the lock releases. State that plainly instead.
- usb.rs's emit_panic_record doc comment ('the one theoretical overlap ... cannot happen in release, where nothing in that region panics') and emit_blocking's 'USB interrupt handler is still installed and still firing' line: after 2a-2c land, verify these are true again (no panic fires inside RECORD_BUFS.lock() anymore) and leave them as-is if so; reword only the specific clause that needs it if not.
- frame.rs's doc section title '# A stall after the pre-check panics, in every profile' (added by TASK-040): update per 2a above so it no longer claims write_whole itself panics.

## 4. Test update

Locate the existing test in frame.rs's `#[cfg(test)] mod tests` (added by TASK-040, currently named write_whole_panics_when_the_sink_stalls_after_the_precheck, using #[should_panic(expected = "sink stalled")]). Replace it with a plain #[test] (no should_panic) that calls write_whole with the same stalling closure and asserts:

    assert_eq!(outcome, frame::WriteOutcome::Stalled);

Rename it to something like write_whole_reports_stalled_when_the_sink_stalls_after_the_precheck, and add a one-line comment noting that the actual panic now happens in the caller (emit()/try_emit_dump() in lib.rs), which only links under the log-usb firmware target and is not host-testable at this layer -- this test's job is to prove the outcome value the caller depends on, not the panic itself.

## 5. Verification (run in this order)

1. nix develop -c cargo fmt --all --check
2. nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings
3. nix develop -c cargo test -p asperitas-logging
4. nix develop -c cargo test --workspace
5. cd firmware && nix develop -c cargo build --release --features seed3
6. git diff --stat firmware/Cargo.lock   # must be empty -- this ticket touches no dependencies

## 6. In the Final Summary

State explicitly: (a) write_whole no longer panics -- it returns WriteOutcome::Stalled and the panic moved to its two callers, firing only after RECORD_BUFS.lock() has released the critical section; (b) why that matters -- this target's panic strategy is abort, so a panic inside a critical_section::with closure never restores PRIMASK, which would have left interrupts masked for the rest of the program's life and silently defeated usb::emit_blocking's assumption that the USB interrupt is still firing during the panic spin, exactly contradicting TASK-040's own stated goal of routing this failure through the fail-loud USB path; (c) the try_emit_dump doc block's interrupts-enabled claim was also corrected to interrupts-masked/capacity-frozen, since it was already false independent of this bug.
<!-- SECTION:PLAN:END -->
