---
id: TASK-045
title: >-
  Fix: write_whole's caller-contract panic can permanently mask interrupts,
  silencing the panic text it exists to deliver
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 07:02'
updated_date: '2026-09-10 08:34'
labels:
  - planned
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
- [x] #1 write_whole (frame.rs) no longer panics itself; it returns a result distinguishing the three outcomes (committed, refused up front for lack of space, sink stalled after partial progress) instead of bool. Its doc comment states plainly that the caller, not write_whole, is responsible for panicking on the stalled outcome, and must do so only after any lock/critical section held during the call has been released.
- [x] #2 Both call sites in crates/asperitas-logging/src/lib.rs (emit() and try_emit_dump()) propagate the stalled outcome out of their RECORD_BUFS.lock(|cell| {...}) closure and panic (message equivalent to today's write_whole stall message) only after that closure has returned -- i.e. outside the critical section. Neither closure panics internally on this path anymore.
- [x] #3 The frame.rs unit test exercising the stall path is updated to assert write_whole's returned Stalled outcome directly (no #[should_panic] on write_whole itself), with a short comment explaining that the panic itself now happens in the caller and is therefore only reachable under the log-usb firmware target, not on host.
- [x] #4 lib.rs's try_emit_dump doc block is corrected: RECORD_BUFS.lock() masks interrupts globally for its duration (CriticalSectionRawMutex), so the consumer cannot run at all while the lock is held and free capacity is frozen (not merely non-decreasing) during the call; the now-false 'write_whole's stall panic cannot fire on this path' claim is replaced with an accurate statement of where that panic fires after this fix.
- [x] #5 usb.rs's emit_panic_record doc comment ('cannot happen in release, where nothing in that region panics') and emit_blocking's 'USB interrupt handler is still installed and still firing' assumption are re-verified true given this fix (panic no longer fires inside RECORD_BUFS.lock()), and reworded only if the fix's exact mechanics require it to stay accurate.
- [x] #6 nix develop -c cargo fmt --all --check passes
- [x] #7 nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings passes
- [x] #8 nix develop -c cargo test -p asperitas-logging passes, including the updated stall-path test
- [x] #9 nix develop -c cargo test --workspace passes and cd firmware && nix develop -c cargo build --release --features seed3 succeeds, with firmware/Cargo.lock showing no diff
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): Rust embedded project (crates/*, firmware/) targeting the Daisy Seed3 (`thumbv7em-none-eabihf`). Prefix every command with `nix develop -c`; work from the repository root. Touch no dependency versions. This ticket is one atomic change — do NOT split it into commits per file; the signature change breaks every call site until all land.

## 0. What this plan changes relative to the review-pass draft

The draft written into this ticket when it was filed is correct about the mechanism and roughly right about the fix, but it has three defects this plan fixes. Do not re-follow the draft where they disagree:

1. **It missed two call sites of `write_whole`.** Besides `lib.rs:346` and `lib.rs:492`, `write_whole` is called by its own test module (`frame.rs:1233-1289`, plus the `commit()` helper at `frame.rs:1303-1312`) and by the integration suite `crates/asperitas-logging/tests/console_dump.rs:1956-1960` (`commit_frame`, used at lines 2072, 2209, 2222, 2284, 2300). Miss those and AC#9's `cargo test --workspace` fails to *compile*.
2. **Its line numbers are stale.** Actual: `frame.rs` doc 283-319 + fn 320-341; `emit()` doc 298-320 + fn 321-355 (call site 346); `try_emit_dump()` doc 409-460 + fn 461-511 (call site 492).
3. **It proposed `(bool, bool)` tuple returns from the lock closures and duplicated the stall panic at both call sites.** Prefer the single-owner shape in §2b below; it puts the "never panic inside the record lock" rule in exactly one place so a future third caller cannot forget it. Inline-at-both-sites remains an acceptable fallback (§2b).

Also added here: `#[must_use]` on the new outcome type (§2a), hoisting one oversized-body check out of the critical section (§2c), the `emit()` inline comment that makes the same false claim as the `try_emit_dump` doc block (§4), and the two residual hazards found while planning, which are ticketed separately and deliberately NOT part of this ticket (§7).

## 1. Facts established by two research passes — do not re-research

- `RECORD_BUFS` = `embassy_sync::blocking_mutex::Mutex<CriticalSectionRawMutex, UnsafeCell<RecordBufs>>`, `lib.rs:289-296`, `#[cfg(feature = "log-usb")]`. Same mutex type on `LOG_PIPE`, `lib.rs:271-275`. The `critical-section` impl is cortex-m's `critical-section-single-core` (`crates/asperitas-logging/Cargo.toml:23`, non-optional; arch-gated inside cortex-m, which is why host builds compile but `log-usb` code fails to *link* there).
- `critical-section::with` releases via a private `Guard`'s `Drop`; upstream documents *"This function panics if the given closure `f` panics. In this case the critical section is released before unwinding."* This target is `panic="abort"` (`rustc --print cfg --target thumbv7em-none-eabihf` → `panic="abort"`; `firmware/Cargo.toml` `[profile.release]` deliberately sets no `panic` key), so there is no unwinding, the guard never drops, `critical_section::release()` never runs, and PRIMASK stays set for the life of the program. Confirmed mechanism, not speculation.
- Do **not** "fix" this by calling `cortex_m::interrupt::enable()` before panicking: it is documented `# Safety: Do not call this function inside an interrupt::free critical section`, and PRIMASK semantics are already contested upstream (rust-embedded/cortex-m#196). Moving the panic outside the lock is the only route inside the published contract.
- Panic flow: `#[panic_handler]` → `asperitas_logging::panic_handler::handle_panic` (`panic_handler.rs:52`) → LED first (`:54`, synchronous GPIO, works masked) → under `log-usb`, `usb::emit_panic_record` (`:66-69`) → `usb::emit_blocking` (`usb.rs:376-424`), whose doc at `usb.rs:364-367` asserts *"the USB interrupt handler is still installed and still firing during the panic spin"* → halt `loop { nop }` (`:93-95`). With PRIMASK stuck set that assumption is false, so the text is silently dropped. That is the bug.
- `emit_blocking`'s timeout is `EMIT_TIMEOUT = 3 s` (`usb.rs:47-51`) compared against `embassy_time::Instant::now()` (`usb.rs:417-419`). See §7 — that clock itself depends on an ISR.
- **Every `emit()` call site today runs on the thread-mode executor with interrupts enabled before entering the lock.** Verified: no logging from any ISR body in `firmware/` or `crates/`; daisy-embassy's "audio callback" is an async loop on the thread executor, not an IRQ; there is no `InterruptExecutor` anywhere in the workspace; `daisy-embassy`'s `info!` in `audio.rs:170` is `defmt::info`, not `log`. The only explicit masking call in firmware is `firmware/src/bin/main.rs:269` `cortex_m::interrupt::free(|_| knob_state.read())`, which logs nothing. So AC#5's premise holds after this fix; scope its wording to "interrupts are live when the panic fires", not "this emitter can wake a masked core".
- `console::CONSOLE.record_committed()` (`console.rs:119`), `record_dropped_for_space(usize)` (`console.rs:125`), `take_seq()` (`console.rs:114`) are `AtomicU32` ops that take **no lock** (`console.rs:77-79`: they exist to be snapshotted without touching the record lock). They may move in or out of the closure freely; only `take_seq()` must stay *inside*, because seq-inside-the-lock is what makes numeric order equal wire order (`console.rs:110-112`).
- Host reachability: `emit()` / `try_emit_dump()` are `#[cfg(feature = "log-usb")]` and do not link on host (`lib.rs:445-454`, `tests/console_dump.rs:2039-2043`). No host test touches them. Therefore the panic itself is not host-testable; asserting the returned outcome is the available substitute — the same substitution `std::sync` poisoning and embassy-sync's explicitly poisoning-free `RawMutex` make, and the standard `#[test]`-needs-`std` split.
- Baseline at HEAD (measured, so failures below are yours): `cargo fmt --all --check` clean, `cargo clippy -p asperitas-logging --all-targets -- -D warnings` clean, `cargo test -p asperitas-logging` green, `cargo test --workspace` green.

## 2. The fix

### 2a. `crates/asperitas-logging/src/frame.rs` — `write_whole` reports, never panics

Replace `-> bool` (fn at 320-341) with a public outcome type matching this crate's enum style (`dump.rs:123,131,535,807,846` all use `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum` with documented variants):

```rust
/// What one [`write_whole`] call did to its sink. Ignoring this value is a bug: the
/// `Stalled` variant is how a broken sink reaches the fail-loud path.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// Every byte of the frame is in the sink, contiguously and in order.
    Committed,
    /// The frame did not fit the capacity pre-check. `write` was called zero times.
    RefusedForSpace,
    /// The sink stopped accepting bytes **after** the pre-check passed — a violation of
    /// the caller's contract. `write_whole` does not panic on it: see the doc section
    /// below for why the panic belongs to the caller, and where it may fire.
    Stalled,
}
```

Body becomes: `RefusedForSpace` on the pre-check, `Stalled` from the `None`/`Some(0)` arm (delete the `panic!` at 334-336), `Committed` on loop exit.

Keep `Stalled` payload-free. Today's message interpolates the sink's return value (`{stalled:?}`); losing that `None` vs `Some(0)` distinction costs nothing actionable and keeps the API honest about not expressing partial writes. The caller's message names the call site instead (§2b).

Rewrite the doc comment (283-319) — two separate paragraphs need it:

- **283-293 (the opening).** It currently claims *"Exactly two outcomes"* and *"There is no third outcome and no partial-write count on purpose"*. Both become false. New framing: three outcomes; what whole-record-or-nothing actually guarantees is that a truncated record is *detectable downstream by the CRC/framing*, not that nothing reached the sink — on `Stalled`, some bytes genuinely did land, and that is precisely the case framing exists to expose rather than hide. Keep the reserve/commit comparison to the Linux ring buffer / `prb_reserve` / bitdrift (still accurate); keep the "# Why the loop is required, not defensive" section (295-304) untouched — the short-write measurement there stands.
- **306-319 (the stall section).** Retitle away from *"A stall after the pre-check panics, in every profile"*. State: `write_whole` never panics; a stall is reported as `WriteOutcome::Stalled`; the caller MUST panic on it — loudly, in every profile, because `debug_assert!` would vanish from the release build that ships — but only **after** whatever lock protected the call has been released. Give the reason inline, since it is the whole point: this target aborts, a panic raised inside a `critical_section::with` closure never restores `PRIMASK`, and interrupting the board permanently on top of silencing the panic text is strictly worse than the crash. Name `emit()` and `try_emit_dump()` as the enforcement points. Keep the existing "retrying is not a milder option" argument (313-316) — it still explains why we do not loop.

### 2b. `crates/asperitas-logging/src/lib.rs` — one owner of the "panic outside the lock" rule

Add one private helper next to `RECORD_BUFS` that owns (a) the `UnsafeCell` acquisition and its safety comment and (b) the stall panic, which by construction can only run after `lock()` has returned normally:

```rust
/// Run `commit` with the record buffers held, and fail loud if the pipe stalled mid-frame.
///
/// The panic lives here, outside `RECORD_BUFS.lock`, and nowhere else. `RECORD_BUFS` is a
/// `CriticalSectionRawMutex`, so its closure runs with `PRIMASK` set, and this target aborts
/// rather than unwinds: a panic raised inside that closure never restores `PRIMASK`, so the
/// panic handler's serial emit (`usb::emit_blocking`) would spin with no USB interrupt and
/// drop the very text it exists to deliver. Callers therefore report `WriteOutcome::Stalled`
/// out of their closure and let this function crash. Returns whether the frame reached the pipe.
#[cfg(feature = "log-usb")]
fn commit_records(commit: impl FnOnce(&mut RecordBufs) -> frame::WriteOutcome) -> bool {
    let outcome = RECORD_BUFS.lock(|cell| {
        // Safety: the only route to these buffers is this mutex, the core is single-core,
        // and the reference never escapes this closure.
        commit(unsafe { &mut *cell.get() })
    });
    if outcome == frame::WriteOutcome::Stalled {
        panic!("record commit: the sink stalled after the capacity pre-check passed; the caller held RECORD_BUFS and verified free_capacity, so the pipe broke write_whole's contract and a record was lost");
    }
    outcome == frame::WriteOutcome::Committed
}
```

`emit()` (321-355) then reads: keep `now_ms` read *before* the lock (existing comment at 323-327 stands), pass the rest as the closure — `take_seq`, `fill`, `encode`, `body_shortened`, then the `write_whole` call with the counter bumps keyed off its outcome — and return `true` for `Committed`, `false` otherwise. Ignore `emit()`'s own bool result (it has already counted the verdict internally).

`try_emit_dump()` (461-511) returns `commit_records(|bufs| { ... })` directly; its early refusals return `false` from the closure as `RefusedForSpace` after doing whatever side effect they do today.

**Machine check for AC#2:** after this lands, no `panic!` may appear lexically inside either lock closure or inside `write_whole`. Verify with
`awk '/RECORD_BUFS.lock|fn write_whole/,/^}/{print FILENAME":"FNR": "$0}' crates/asperitas-logging/src/{lib,frame}.rs | grep -n 'panic!'` → expect zero hits (`commit_records`'s panic sits after the `lock(...)` call, so it is not caught).

**Fallback, if the generic closure fights a borrow:** keep `RECORD_BUFS.lock(|cell| …)` open-coded at both call sites exactly as the draft proposed, returning `frame::WriteOutcome` (not a tuple) from each closure, and put the identical post-lock panic behind one `#[cold] #[inline(never)] fn stall_panic() -> !` so the message and its rationale still have one owner. Choose this only on technical necessity, and say which you took in the Final Summary.

### 2c. `try_emit_dump()` — hoist the oversized-body check out of the critical section

`lib.rs:470-477` validates `body.len() > frame::MAX_BODY` — pure input validation over an argument, no shared state — and then `debug_assert!(false, …)` **inside** the lock, i.e. another debug-profile panic that would leave PRIMASK stuck. Move the check and its `debug_assert!` to just after `let now_ms = …` (before the lock) and return `false` there. Behaviour in release is unchanged (the assert compiles away); behaviour in debug strictly improves, and it removes one more in-lock panic site. Leave the other two `debug_assert!`s inside (they concern values computed under the lock) and note them as residual (§7).

## 3. Call-site updates the draft missed

- `frame.rs` test module: `write_whole_refuses_a_frame_that_does_not_fit_without_writing_anything` (1233-1251) — the `assert!(!ok)` becomes `assert_eq!(…, WriteOutcome::RefusedForSpace)` and the empty-frame `assert!(write_whole(&[], 0, …))` becomes an equality against `Committed`; `write_whole_accepts_only_after_every_byte_reaches_the_sink` (1253-1264) — equality against `Committed`; the `commit()` helper (1303-1312) keeps its `-> bool` external shape by wrapping: `matches!(write_whole(…), WriteOutcome::Committed)`, so the ring-wrap test (1349-1379) and the randomized rounds test (1381-1437) need no edits at all. Import `WriteOutcome` in the test module.
- `tests/console_dump.rs`: extend the import at 1933 to bring `WriteOutcome`, and wrap the `commit_frame` body (1956-1960) the same way. Its five uses (2072, 2209, 2222, 2284, 2300) then need no changes.
- Grep confirms no other source references: `docs/` and `firmware/` have none. Completed tickets' text (`task-030 …:241` "two outcomes and no third") is historical record — leave it alone.

## 4. Doc corrections (exact locations)

1. `frame.rs:283-293` and `306-319` — per §2a.
2. `lib.rs:342-345` (inline comment in `emit`) — *"inside, the consumer can only ever increase free capacity"* is wrong for the same reason as item 3: the consumer is a thread-mode task and cannot run at all behind PRIMASK. Say capacity is **frozen**, which is the stronger invariant and is what actually makes the pre-check sound.
3. `lib.rs:422-428` (`try_emit_dump` doc) — replace *"While the lock is held the consumer still runs — it runs with interrupts enabled, so free capacity can only grow here"* with: `RECORD_BUFS.lock` masks interrupts globally, the consumer (`usb::run()`'s drain task) cannot run at all while it is held, so free capacity is frozen for the duration — which is what makes the capacity check sound rather than optimistic. Replace the trailing *"and it is why `write_whole`'s stall panic cannot fire on this path"* with the truth after this fix: `write_whole` no longer panics at all; a stall surfaces as an outcome and `commit_records` crashes on it once the critical section has released.
4. `usb.rs:337-339` (`emit_panic_record`) — *"the one theoretical overlap (a panic raised inside the commit critical section) cannot happen in release, where nothing in that region panics"*: re-verify true after §2b/§2c and tighten the wording to name the mechanism ("callers crash only outside the record lock"), since "nothing in that region panics" is a claim about absence that the next debug_assert would silently break.
5. `usb.rs:364-367` (`emit_blocking`) — narrow the claim to what it actually relies on: it *assumes interrupts are still live when it runs; it does not make them live*. One sentence of contrast is worth adding while here: the `log-defmt` sibling is immune for a different reason (RTT is polled by the probe, `docs/reference/daisy-seed3.md:422` records the opposite failure mode for a stalled RTT host), which is why the USB path is the fragile twin. Do not promise a mask-proof CDC emitter — §7 tickets it.
6. `panic_handler.rs:61-64` repeats the "USB interrupt is still firing" phrasing outside `usb.rs`; keep it consistent with item 5 in the same pass.
7. `lib.rs:46` intra-doc link to `frame::write_whole` and any new link to `WriteOutcome` must resolve. Pre-existing broken intra-doc links in this crate are TASK-043's problem — do not fix unrelated ones, but do not add new ones.

## 5. Test update (AC#3)

Replace `write_whole_panics_when_the_sink_stalls_after_the_precheck` (doc 1265-1272, `#[should_panic(expected = "sink stalled")]` at 1273-1274, body 1275-1289) with a plain `#[test] fn write_whole_reports_stalled_when_the_sink_stalls_after_the_precheck` reusing the same two-round stalling closure verbatim (round 1 `Some(chunk.len() / 2)`, round 2 `Some(0)`) and asserting `assert_eq!(outcome, WriteOutcome::Stalled)` plus `assert_eq!(calls, 2)` so the test still proves the loop was genuinely mid-frame when the sink stopped.

Rewrite its doc comment: the old one argues "`should_panic` is the whole point of this test … loud in both profiles". New rationale: the assertion here is the *value* the callers branch on; the panic now lives in the caller (`commit_records`, `lib.rs`) which does not even link on host, so host cannot see the crash and must not try. Keep the sentence recording *why* the old `debug_assert!` was rejected (silent in release) — that history is load-bearing.

With `#[must_use]` on `WriteOutcome` and AC#7's `-D warnings`, forgetting to consume the outcome at any call site becomes a build failure, which is the point.

## 6. Verification, in this order

1. `nix develop -c cargo fmt --all --check`
2. `nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings`
3. `nix develop -c cargo test -p asperitas-logging` (includes the renamed stall test)
4. `nix develop -c cargo test --workspace` (catches `tests/console_dump.rs` — the suite most likely to break if §3 is skipped)
5. `cd firmware && nix develop -c cargo build --release --features seed3`
6. `git diff --stat firmware/Cargo.lock` → must be empty
7. The AC#2 machine check in §2b.

## 7. Residual hazards — named, ticketed elsewhere, deliberately NOT this ticket

Both were found while planning and are untracked anywhere else. File references are verified.

- **TASK-046** — `emit_blocking`'s `EMIT_TIMEOUT` bound depends on an interrupt it cannot restore. `embassy-stm32 0.6.0`'s GP16 time driver computes `now()` as `(period << 15) + hardware CNT` (`time_driver/gp16.rs:81-82`, `:347-353`) and increments `period` **only in the timer ISR** (`next_period`, `:193-199`). With PRIMASK set, `period` freezes while the 16-bit counter free-runs, so `Instant::now()` goes non-monotonic and the `while Instant::now() < deadline` comparison at `usb.rs:419` can stop terminating — contradicting `EMIT_TIMEOUT`'s own "rather than spinning here forever" (`usb.rs:50`). Post-fix the moved panic never reaches this, but any panic that *does* fire under a mask (including the debug-only asserts below) still can. Fix belongs in a cycle-counted bound, not here.
- **TASK-047** — the `debug_assert!`s that stay inside the `try_emit_dump` closure even after §2c (`lib.rs:486-489`, `:499-503`), plus those inside `frame::encode`, would leave PRIMASK stuck the same way. Debug-profile only, so the shipping release image is unaffected; worth routing through the same outcome-after-lock pattern.
- Hardware proof of the stall path is out of scope and not achievable yet: `firmware/src/bin/panictest.rs` can only raise a plain panic (`:181`) and has no fault-injection hook for a sink stall, so no person can reproduce the scenario by flashing a binary. The general "is the framed console legible on hardware" confirmation is already TASK-030.04 (`@human`); do not duplicate it, and do not mark anything in this ticket `HUMAN:`.

## 8. In the Final Summary

State explicitly: (a) `write_whole` no longer panics — it returns `WriteOutcome::Stalled`, and the panic now fires only after `RECORD_BUFS.lock` has returned, owned by one helper rather than duplicated per call site (say which shape you took if you used the §2b fallback); (b) why that matters — `panic="abort"` means a panic inside `critical_section::with` never runs the guard's `Drop`, so PRIMASK stays set forever and `usb::emit_blocking` spins with no USB interrupt, dropping the panic text for exactly the path TASK-040 added fail-loud handling for; (c) that two docs claiming the consumer runs with interrupts enabled *while the record lock is held* (`lib.rs:342-345`, `lib.rs:422-428`) were independently wrong and are corrected to capacity-frozen; (d) the four call sites updated, including the two test helpers; (e) that TASK-046/TASK-047 exist for the two residual hazards in §7, and that hardware confirmation of this path needs a fault-injection hook nobody has built.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented per plan $2a/$2b/$2c/$3/$4/$5 (primary shape, not the fallback): frame::write_whole now returns #[must_use] WriteOutcome {Committed, RefusedForSpace, Stalled} and never panics; lib.rs gained one private commit_records() that owns the UnsafeCell acquisition and the sole stall panic, which by construction fires only after RECORD_BUFS.lock() has returned. emit() and try_emit_dump() both route through it and report Stalled out of their closures; neither closure panics on that path. Oversized-body check + its debug_assert hoisted out of the critical section (plan 2c); the two remaining in-lock debug_asserts in try_emit_dump stay and belong to TASK-047. Call sites updated: frame.rs tests (3 renamed/rewritten + commit() helper wrapped in matches!), tests/console_dump.rs commit_frame wrapped, import extended with WriteOutcome. Docs corrected: frame.rs write_whole opening + stall section (three outcomes; Stalled may leave bytes in the sink, detectable via CRC/framing; caller must panic only post-lock, abort/PRIMASK rationale inline); lib.rs module record-path blurb, emit()'s inline capacity comment and try_emit_dump's 'consumer still runs' bullet both corrected to capacity-FROZEN (CriticalSectionRawMutex masks interrupts globally); usb.rs emit_panic_record wording names the mechanism (record-path callers crash only outside the record lock; debug-only asserts inside are TASK-047); usb.rs emit_blocking narrowed to 'assumes interrupts are live, does not make them live' + log-defmt contrast; panic_handler.rs aligned. Machine check note: the plan's awk heuristic reports one false positive (its range starts at commit_records' RECORD_BUFS.lock(|cell| line and only ends at a column-0 }, so it swallows the intended post-lock panic). Stronger exact check instead: grep shows exactly ONE RECORD_BUFS.lock call site in code (lib.rs:310, inside commit_records) whose 3-line closure contains no panic!, and lib.rs contains exactly one panic! total, positioned after the lock's closing }); . No panic! remains lexically inside any lock closure or write_whole.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
write_whole no longer panics: it returns WriteOutcome::Stalled, and the panic now fires only after RECORD_BUFS.lock has returned, owned by a single helper (commit_records) rather than duplicated per call site - the plan's primary shape, not the fallback. Why it matters: this target is panic=abort, so a panic inside critical_section::with never runs the guard's Drop, PRIMASK stays set for the life of the program, and usb::emit_blocking spins its full EMIT_TIMEOUT with no USB interrupt - dropping the panic text for exactly the path TASK-040 added fail-loud handling for, leaving only the LED. Two docs claiming the drain consumer runs with interrupts enabled while the record lock is held (emit()'s inline comment, try_emit_dump()'s doc bullet) were independently wrong and are corrected to free-capacity-frozen, the stronger invariant that actually makes the pre-check sound. Four write_whole call sites updated: emit(), try_emit_dump(), frame.rs's commit() test helper, and tests/console_dump.rs's commit_frame (the two the review draft missed); the oversized-body check was hoisted out of the critical section, removing one more in-lock panic site. Verified: fmt/clippy(-D warnings)/crate tests/workspace tests green incl. the renamed host test asserting the Stalled outcome directly; firmware release build with seed3 succeeds and firmware/Cargo.lock shows no diff. Residual hazards stay ticketed: TASK-046 (EMIT_TIMEOUT clock can go non-monotonic under a mask) and TASK-047 (remaining debug_asserts inside the lock, debug-profile only). Hardware confirmation of the stall path still needs a fault-injection hook nobody has built; nothing here is markable HUMAN:.
<!-- SECTION:FINAL_SUMMARY:END -->
