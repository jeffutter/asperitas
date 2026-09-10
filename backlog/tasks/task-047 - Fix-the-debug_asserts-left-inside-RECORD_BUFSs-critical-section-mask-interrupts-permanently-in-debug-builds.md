---
id: TASK-047
title: >-
  Fix: the debug_assert!s left inside RECORD_BUFS's critical section mask
  interrupts permanently in debug builds
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-10 08:09'
updated_date: '2026-09-10 16:25'
labels:
  - planned
dependencies:
  - TASK-045
priority: low
type: bug
ordinal: 74500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while planning TASK-045, which fixes the release-profile instance of this same fault class.

TASK-045 moves `write_whole`'s stall panic out of the `RECORD_BUFS.lock(|cell| …)` closures, because `RECORD_BUFS` is a `CriticalSectionRawMutex` and this target is `panic="abort"`: a panic raised inside a `critical_section::with` closure never runs the guard's `Drop`, so `PRIMASK` stays set for the remaining life of the program and `usb::emit_blocking` then spins with no USB interrupt and silently drops the panic text it exists to deliver.

That argument applies verbatim to every other panic still inside those closures. After TASK-045 lands, `try_emit_dump` keeps at least two `debug_assert!`s whose condition is about values computed under the lock (`encoded.truncated`, and the "pipe refused a frame the headroom rule already admitted" refusal), and `frame::encode` asserts internally too. Each is debug-profile only, so the shipping release image is unaffected — but a debug build that trips one goes deaf permanently rather than printing where it died, which is precisely the failure mode TASK-045 exists to remove, and it will bite whoever debugs the dump path on the bench.

Work: convert the remaining in-closure panics into reported outcomes checked after the critical section releases, following whatever shape TASK-045 settled on (read its Final Summary and the resulting `commit_records`/outcome code first — do not invent a second pattern). Where an assert genuinely cannot be expressed as a returned value without leaking internals, say so in the doc comment instead of leaving a bare `debug_assert!`, and keep the machine check TASK-045 introduces (`no panic!/assert expression lexically inside a RECORD_BUFS.lock closure`) green by extending it to cover asserts.

Acceptance criteria are deliberately device-free: fmt, `clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test --workspace`, and the seed3 release build all pass with `firmware/Cargo.lock` unchanged, plus a host test per converted site asserting the reported outcome rather than the crash.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both `commit_records(|bufs| ...)` closures (`emit()` and `try_emit_dump()`) contain no panic-forming macro; the "pipe refused a frame the headroom rule already admitted" case is reported out of the closure as a value returned beside `frame::WriteOutcome` -- deliberately NOT a fourth `WriteOutcome` variant and NOT a parallel outcome type -- and crashed on inside `commit_records` only after `RECORD_BUFS.lock` has returned, with today's message text preserved.
- [ ] #2 The tautological `debug_assert!(!encoded.truncated, ...)` is deleted (the pre-lock `body.len() > MAX_BODY` guard makes it unreachable), replaced by a comment naming that guard and a host test pinning `Encoded::truncated == (body.len() > MAX_BODY)` on both sides of the boundary.
- [ ] #3 Release behaviour is unchanged on both paths -- same counters bumped under the lock, same return values, same wire bytes -- and the post-lock check deliberately stays debug-profile-only, with the reason it diverges from TASK-045's unconditional stall panic written into `commit_records`' doc comment.
- [ ] #4 `write_hex` and `write_decimal` carry no runtime assert; every hex/decimal field width in frame.rs and dump.rs is pinned at compile time through one shared const helper owned by frame.rs; `T_MS_WRAP`'s fit is const-checked; prefix/trailer offsets derive from named constants asserted to tile `PREFIX_LEN`/`TRAILER_LEN`; emitted bytes are unchanged.
- [ ] #5 A durable machine check exists at `crates/asperitas-logging/tests/commit_path_no_panic.rs` (no such artifact exists at HEAD today), run by default-feature `cargo test --workspace`, that scans every `commit_records(` argument closure plus the transitively reachable encoder/console/dump functions for panic-forming tokens, asserts exactly one `RECORD_BUFS.lock(` call site, requires the post-lock `panic!` and `debug_assert!` to still exist so deletion cannot satisfy it, carries positive controls so a broken scanner cannot pass vacuously, and trips when a new locally-defined callee appears inside a lock closure.
- [ ] #6 That check was written BEFORE the conversions and observed failing on every site in the plan's inventory (four lexical sites: lib.rs:502, lib.rs:518, frame.rs:264, frame.rs:277); if the first red run reports a different set, reconcile the inventory before converting anything, and record the failure list in Implementation Notes.
- [ ] #7 A host test per converted site asserts the reported outcome rather than the crash -- the classification function's four-row truth table under default features -- and each such test's doc comment states why the crash itself is not host-observable (undefined `__critical_section_1_0_*` / `__embassy_time_now` at link time).
- [ ] #8 The pre-existing `manual_is_multiple_of` violation at usb.rs:421 is fixed, `nix develop -c cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings` passes, and that gate is added to both `.github/workflows/ci.yml` and `lefthook.yml`.
- [ ] #9 All existing gates pass: `cargo fmt --all --check`; `cargo clippy -p asperitas-logging --all-targets -- -D warnings`; `cargo clippy --workspace --all-targets -- -D warnings` with and without `--features asperitas-pod/pod-hw`; `cargo test -p asperitas-logging`; `cargo test --workspace`.
- [ ] #10 `cd firmware && nix develop -c cargo build --release --features seed3` and `nix develop -c make clippy FEATURES=seed3` succeed, and `git --no-ext-diff diff --stat` shows no change to `Cargo.lock` or `firmware/Cargo.lock`.
- [ ] #11 Docs are corrected per the plan's doc section (commit_records, try_emit_dump, write_hex/write_decimal, usb::emit_panic_record) with no NEW rustdoc warnings from `cargo doc -p asperitas-logging --no-deps` under default features or `boot-led,log-usb,log-defmt`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): Rust embedded project (`crates/*`, separate `firmware/` workspace) targeting the Daisy Seed3 (`thumbv7em-none-eabihf`, `panic="abort"`). Prefix every cargo command with `nix develop -c` and run from the repository root. Touch no dependency versions and add no dependency — everything needed is already in the tree. This is one atomic change: `commit_records`' closure signature, both call sites, the codec's width constants, and the new test all land together. **Do the steps in the order below** — Step 1 deliberately fails until Steps 2–4 land, and that red→green transition is the proof that the durable check has teeth.

## 0. What this plan decides, and where it departs from the ticket draft

**No sub-tickets.** The three pieces (convert the asserts, const-enforce the widths, commit the machine check) are ordered phases of one fix: a machine check written against the unconverted code is worthless, and the conversions without the check are exactly the drift TASK-045 suffered (its "machine check" was a grep pasted into prose and never became an artifact). Splitting them would create a child that cannot pass until its parent's work exists — backwards under this repo's children-block-parents rule.

**Nothing in this ticket is `@human`.** Every criterion is satisfiable on the host, and there is no bench step to split out: tripping either violation on hardware needs a fault-injection hook that does not exist (TASK-045 §7 records the same conclusion for the stall path), and "is the framed console legible on hardware" is already TASK-030.04. Do not add `HUMAN:` criteria, and do not mark anything Done on the strength of a compiling build — the checks in §7 are the evidence.

Four departures from the draft text, each justified below. Do not re-follow the draft where they disagree:

1. **`debug_assert!(!encoded.truncated)` (lib.rs:502) is deleted, not converted.** It is a tautology: `Encoded::truncated` *is* `body.len() > MAX_BODY` (frame.rs:222) and lib.rs:485 already returned early when `body.len() > frame::MAX_BODY` before taking the lock. Plumbing a bit that provably cannot be set buys a code path nobody can execute. Instead: delete it, name the reason in a doc sentence, and pin the equivalence with a host test (§5).
2. **The `frame::encode`-side asserts become compile-time facts, not outcomes.** `write_hex`'s width assert and `write_decimal`'s fits assert are functions of constants (§4). Returning them as outcomes would leak codec internals into a public API to report states that cannot occur, and would force a decision on `usb.rs:351`, which calls `encode` from the panic handler outside the lock and must stay able to ignore any new field. This is the ticket's own escape clause ("say so in the doc comment instead of leaving a bare `debug_assert!`"), taken one step further: the compiler says it, so the doc comment only has to explain why no runtime check remains.
3. **The post-lock crash stays debug-profile-only**, unlike TASK-045's unconditional stall panic. Deliberate, and §2c argues it.
4. **The machine check becomes a committed test file**, not a grep in ticket prose (§2).
5. **The value carried out of the closure is an opaque violation carrier, returned alongside `frame::WriteOutcome`** — not a new `WriteOutcome` variant and not a parallel outcome type. A review pass of this plan asked for exactly that distinction, and it is right: `WriteOutcome` already says what the sink did, and a second type that also answers "did this commit work?" fragments one concept across two vocabularies (§3b).

## 1. Facts established by research and measurement — do not re-research

Verified at HEAD `14b32cd`. Line numbers are current; re-check them only if a prior step shifted the file.

- **The hazard is real and mechanism-confirmed.** `RECORD_BUFS` (lib.rs:279-287) is `embassy_sync::blocking_mutex::Mutex<CriticalSectionRawMutex, UnsafeCell<RecordBufs>>`; `.lock()` is `critical_section::with`, whose guard restores state only in its `Drop`. This target is `panic="abort"`, so a panic raised inside that closure never restores `PRIMASK`. `usb::emit_blocking` then spins assuming interrupts are live (a claim TASK-046 narrowed to "assumes interrupts are live; does not make them live") and the panic text is dropped.
- **TASK-045's funnel moved the closures one level out, and that breaks the inherited grep.** The only `RECORD_BUFS.lock(` in the workspace is inside `commit_records` (lib.rs:297-308), whose 3-line forwarding closure is already clean. The real bodies live in the `commit_records(|bufs| { … })` arguments at lib.rs:341 (`emit`) and lib.rs:494 (`try_emit_dump`). A check that greps `RECORD_BUFS.lock` passes **vacuously today**. Any check that does not scan `commit_records(` call sites does not satisfy this ticket.
- **Exactly four panic sources remain under the lock**, all debug-profile except as noted:
  | site | condition | status |
  |---|---|---|
  | lib.rs:502 `debug_assert!(!encoded.truncated, …)` | `body.len() > MAX_BODY`, already excluded at lib.rs:485 | tautology → delete (§3a) |
  | lib.rs:518 `debug_assert!(false, "pipe refused a {}-byte frame the headroom rule had already admitted", framed.len())` | reached when `write_whole` returns `RefusedForSpace` after `dump::dump_fits` passed (lib.rs:496) | genuine runtime contract break → report (§3b) |
  | frame.rs:264 `debug_assert!(dst.len() <= 8, "hex field wider than a u32")` | widths are literal ranges at the call sites (`out[3..11]`, `out[trailer+1..trailer+5]`) | const-decidable → §4 |
  | frame.rs:276-277 `let fits = value < 10u32.pow(dst.len() as u32); debug_assert!(fits, …)` | `value = now_ms % T_MS_WRAP`, `T_MS_WRAP = 100_000_000` (frame.rs:121), 8 digits | const-decidable → §4. Note the *guard itself* is a latent all-profile panic: `u32::pow` overflows for `dst.len() >= 10`, and it runs before the assert. Deleting the assert deletes the computation. |
- **Reachable-but-clean, verified**: `write_whole` (frame.rs:354-386, no panics since TASK-045), `level_letter`, `sanitize_byte`, `crc16_ccitt`, `dump::dump_fits` (pure const fn), and `console::{take_seq, record_committed, record_dropped_for_space, body_shortened}` (checked_add + `unwrap_or(u32::MAX)`, no locks). `LOG_PIPE.{free_capacity, try_write}` are `.ok()`-wrapped.
- **Out of scope, classified so nobody refactors it**: `frame.rs:600, 671, 828` are decoder-side; `grep -rn "Decoder" crates firmware` finds it only in `tests/console_frame.rs`, `tests/console_dump.rs`, `examples/console_decode.rs` and in-source tests — never in `firmware/`. `dump.rs:260, 314, 353, 627, 906` are base64/assembler paths; `try_emit_dump` and the dump builders still have **no device caller** (TASK-038.03 owns wiring them). When they are wired, they inherit this rule — record that in the Final Summary, do not fix it now. `usb.rs:159` (`panic!("USB logging already initialized")`) runs on the boot path with no lock held.
- **Host reachability is settled by measurement, not folklore.** An integration test that actually calls `try_emit_dump` under `--features log-usb` **fails to link on host**: `Undefined symbols … __critical_section_1_0_acquire, __critical_section_1_0_release, __embassy_time_now` (measured at HEAD with a throwaway `tests/tmp_probe.rs`, since deleted). The existing `cargo test -p asperitas-logging --features log-usb` binaries link only because nothing references those symbols and `-dead_strip` drops them. Consequence: **no host test can ever execute these closures.** Satisfy "a host test per converted site" by testing the pure value, under default features, exactly as `dump::dump_fits` documents ("the whole headroom rule is decidable on the host while the code that acts on it stays on the device") and as frame.rs:1305-1316 already says about the stall test.
- **Integration tests cannot see `cfg(test)` items.** The `#[cfg(any(feature = "log-usb", test))]` idiom already used for `spin_budget` (lib.rs:213-214) helps only *in-source* `#[cfg(test)] mod tests`. There is no test module in `lib.rs` today; Steps 2/5 add one, and CI's default-feature `cargo test --workspace` runs it.
- **Gate visibility trap: no existing gate compiles the changed code with warnings fatal.** `ci.yml` and `lefthook.yml` run `clippy --workspace --all-targets -- -D warnings` with default features (`log-usb` off), and `cd firmware && cargo build --release --features seed3` without `-D warnings`. Measured consequence: `cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings` **fails at HEAD** with exactly one violation — `usb.rs:421` `manual implementation of .is_multiple_of()`. It is pre-existing, invisible to every current gate, and one line wide. Fix it (§6a) so the new gate can be switched on; a check that cannot be turned on is how TASK-045's check died.
- `cargo build -p asperitas-logging --features log-usb --all-targets` succeeds on host (compile-only, catches type errors in the gated code). Use it as the fast inner loop.
- Baseline green today (measured): `clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test -p asperitas-logging` (38 in-source unit + 38 `console_dump` + 33 `console_frame` integration + 1 doctest, all green), `cargo fmt --all --check`.
- **No durable machine check exists at HEAD `14b32cd`, despite a review pass claiming one.** That review cited `crates/asperitas-logging/tests/commit_path_no_panic.rs` landed by commit `70f390e` ("TASK-045 follow-up"). Verified absent: `git cat-file -t 70f390e` → *not a valid object*; the filename appears in no commit reachable from any ref, in the `/private/tmp/asperitas-base` worktree, or in any of the 37 dangling commits (`git fsck --lost-found`); TASK-045's Implementation Notes record only manual greps and admit its awk heuristic had a false positive. This ticket therefore still has to **create** the artifact. Keep the reviewer's filename (`commit_path_no_panic.rs`) so that if such an artifact ever does land from another session, there is one file and not two. Re-run the same three checks before writing §2 — if the file turns out to exist after all, extend it rather than adding a peer.
- **Blast radius, counted.** `Encoded` is constructed in exactly one place (frame.rs:220); no struct-literal pattern, no destructuring, no whole-value equality anywhere — consumers read `.len` (43 sites) or `.truncated` (13 sites). So *adding* a field is source-compatible; changing `encode`'s return type is not. `WriteOutcome` is exhaustively matched in exactly two places (lib.rs:359-367, lib.rs:511-527); **do not add variants** — it describes what the *sink* did, and a caller-side invariant flag does not belong in it. `commit_records` is private with exactly two callers.
- Repo conventions: 17 prior `const _: () = assert!(…)` pins (frame.rs:114-115, dump.rs:474-483, :1067, knob.rs:86) — this is the house idiom for "the compiler enforces the documented number". Zero uses of `#[cfg(debug_assertions)]` anywhere; do not introduce any (§2c). No `[lints]` section, no `clippy.toml`, `lib.rs` carries only `#![no_std]`, so `-D warnings` comes purely from the command line.
- Gotcha: this repo sets `diff.external` to difftastic, so any `git diff | grep` guard is vacuous unless you pass `--no-ext-diff`.

## 2. Step 1 — the machine check, written first so it goes red

### 2a. Home and shape

New file `crates/asperitas-logging/tests/commit_path_no_panic.rs`, a peer of `console_frame.rs` and `console_dump.rs`. Plain `#[test]`s, no cfg gates, no new dependencies: it reads its own crate's sources as text, so it is feature-independent and runs under CI's ordinary `cargo test --workspace` (and lefthook's pre-push) with no config change. Text access is proven mechanics here: `crates/asperitas-cli/tests/golden_tests.rs:17` already uses `env!("CARGO_MANIFEST_DIR")`.

```rust
const LIB_RS: &str = include_str!("../src/lib.rs");
const FRAME_RS: &str = include_str!("../src/frame.rs");
const CONSOLE_RS: &str = include_str!("../src/console.rs");
const DUMP_RS: &str = include_str!("../src/dump.rs");
```

Two deep helpers carry the whole design; keep them that way rather than sprawling inline:

- `fn masked(src: &str) -> String` — a copy of `src` with every line comment, block comment and string literal replaced by spaces of the same length (so byte offsets and line numbers are preserved). Everything downstream searches the masked text, which is what stops a panic-word inside a doc sentence from failing the build.
- `fn balanced(src: &str, marker: &str) -> Region` — the text from the `{` following `marker` through its matching `}`, using a brace-depth walk over the masked copy, returning the slice plus its start line. Handle `"…"` with backslash escapes and raw strings; treat `'` as ordinary, and **prove that simplification safe rather than assuming it**: assert none of the four sources contains `'{'` or `'}''`.

If brace-matching ever proves brittle, narrow the *list of scanned functions* — never the token list, never the positive controls — and record what you narrowed in Implementation Notes. A check that scans less of the call graph is honest and visible; a check that stops looking for `debug_assert!` is this ticket's failure mode all over again.

### 2b. What it asserts

1. **Positive control per region.** Each extracted region must contain a known substring (`try_emit_dump`'s closure contains `console::CONSOLE.take_seq()`; `commit_records`' definition contains `RECORD_BUFS.lock(`; `fn encode` contains `write_decimal`). A scanner bug that returns an empty region would otherwise pass every scan vacuously — this is the single most important line in the file.
2. **Exactly one `RECORD_BUFS.lock(` occurrence** in the masked `LIB_RS` (pins the `commit_records` funnel: a second, bypassing lock is the bug this whole family of tickets is about), and at least two `commit_records(` call sites besides the definition.
3. **Forbidden-token scan** over (a) every `commit_records(` argument closure region, (b) the interior of `commit_records`' own `RECORD_BUFS.lock(` region, and (c) each function in the reachability list: `fn encode`, `fn write_hex`, `fn write_decimal`, `fn write_whole`, `fn level_letter`, `fn sanitize_byte`, `fn crc16_ccitt` (frame.rs), `fn dump_fits` (dump.rs), `fn take_seq`, `fn record_committed`, `fn record_dropped_for_space`, `fn body_shortened` (console.rs). Forbidden tokens: `panic!`, `assert!`, `assert_eq!`, `assert_ne!`, `debug_assert!`, `debug_assert_eq!`, `debug_assert_ne!`, `unreachable!`, `todo!`, `unimplemented!`, `.unwrap(`, `.expect(`. Fail with the offending `FILE:LINE` and a message naming the rule and pointing at `commit_records`' doc comment.
4. **Anti-deletion guard.** The `commit_records` definition region, *after* the end offset of its `RECORD_BUFS.lock(` region, must contain at least one `panic!` **and** at least one `debug_assert!`. The check cannot be satisfied by deleting the crashes.
5. **Tripwire.** Collect identifiers followed by `(` inside each scanned region; for any that also appears as `fn <name>` in one of the four sources and is not in the scanned list above, fail with "new callee `<name>` reached from the record lock — scan it or prove it panic-free". Cheap insurance against the exact transitive-blindness that hid `write_hex` this long. Pure data constructors reached from a closure (`Violation::noted`) belong on an explicit allowlist with a comment saying why — their bodies are assignments.
6. **Honest limitation, stated in the file header:** a lexical scan sees explicit panic macros, not implicit ones (indexing, shifts, `u32::pow`, integer overflow). That is precisely why Step 3 turns the encoder's runtime checks into compile-time ones rather than deleting them, and why the header must say so.

### 2c. Run it before writing Step 2 and confirm it is red

`nix develop -c cargo test -p asperitas-logging --test record_lock_invariants` must fail listing **exactly** lib.rs:502, lib.rs:518, frame.rs:264, frame.rs:277. Paste that failure list into Implementation Notes; it is the artifact proving the check detects the real defect. If it reports anything else, either the scanner is wrong or the inventory in §1 is incomplete — resolve that before converting anything.

## 3. Step 2 — `crates/asperitas-logging/src/lib.rs`

### 3a. Delete the tautology (lib.rs:502-505)

Remove the `debug_assert!` block. Leave in its place a short comment at the `frame::encode` call: truncation is impossible because the `body.len() > frame::MAX_BODY` guard at the top of `try_emit_dump` runs *before* the lock, and `Encoded::truncated` is exactly that predicate. The equivalence is pinned by a host test (§5), not asserted on the device.

### 3b. Report the one real violation out of the lock

`WriteOutcome` keeps its meaning — what the *sink* did — and stays the only answer to "did this commit work?". The violation travels beside it as an opaque carrier, so converting the next assert means adding a construction site, not editing `commit_records` again. Add, immediately above `commit_records`, gated `#[cfg(any(feature = "log-usb", test))]` (the `spin_budget` idiom at lib.rs:213 — this is what keeps the pure part host-testable without dead-code warnings under default features):

```rust
/// An internal contract break noticed **while the record lock was held**, which must not be
/// raised there: this target aborts, so the panic would never restore `PRIMASK` and the panic
/// handler's serial emit would drop its own text. See [`commit_records`], the only reader.
///
/// Opaque on purpose — the way to consume one is to hand it to `commit_records`, never to
/// branch on it. `frame::WriteOutcome` stays the single vocabulary for what a commit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Violation(Option<(&'static str, usize)>);

impl Violation {
    const NONE: Self = Self(None);

    /// A contract break naming itself and the size involved. Debug builds crash on it once the
    /// record lock has released; release builds keep doing whatever they do today, which for
    /// every current site is counting the loss and returning false.
    const fn noted(msg: &'static str, amount: usize) -> Self {
        Self(Some((msg, amount)))
    }
}

/// Did the pipe refuse a frame the headroom rule had already admitted? Pure and ungated so the
/// whole decision is decidable on the host, like `dump::dump_fits`; the code that acts on it
/// stays behind `log-usb`.
fn refusal_after_admission(
    admitted: bool,
    outcome: frame::WriteOutcome,
    framed_len: usize,
) -> Violation {
    match (admitted, outcome) {
        (true, frame::WriteOutcome::RefusedForSpace) => Violation::noted(
            "pipe refused a frame the headroom rule had already admitted",
            framed_len,
        ),
        _ => Violation::NONE,
    }
}
```

`commit_records` then takes `impl FnOnce(&mut RecordBufs) -> (frame::WriteOutcome, Violation)` and, after the lock has returned, checks the carrier **before** the existing stall panic:

```rust
if let Some((msg, amount)) = violation.0 {
    debug_assert!(false, "record commit invariant violated under RECORD_BUFS: {msg} ({amount} bytes)");
}
```

Keep the stall `panic!` and its message untouched. `emit()`'s closure ends with `(outcome, Violation::NONE)`; `try_emit_dump()`'s ends with `(outcome, refusal_after_admission(admitted, outcome, framed.len()))`, where `admitted` is true only past the headroom check — the up-front refusal at lib.rs:496-498 must return `Violation::NONE`, and that ambiguity is precisely why the bit cannot ride on `WriteOutcome` (adding a fourth variant there would also break the two exhaustive matches at lib.rs:359-367 and :511-527).

Two deliberate choices worth defending if a reviewer pushes back:

- **No `#[cfg(debug_assertions)]` split type.** An opaque carrier whose payload exists only in debug costs zero in release, but defines one type twice and makes `commit_records` reason about two profiles. The repo has zero uses of that cfg; the price here is one `Option` store and one never-taken branch per commit on a path that is not the audio callback. Take the simplicity. Say so in the doc comment.
- **The message loses its `{}`-formatted byte count inside the original sentence.** It becomes `... admitted (128 bytes)`. If exact-text fidelity matters more than a generic carrier, store `(&'static str, usize)` as today and format at the panic site — do not reach for `format_args!` in a const context.

Counter bumps (`record_committed`, `record_dropped_for_space`) stay **inside** the closures exactly where they are: they are the wire ledger taken in the same IRQ-off window, and moving them would change what STATUS reports.

### 3c. Why the post-lock check is `debug_assert!` and not the unconditional panic TASK-045 chose

Say this in the doc comment, because it looks like an inconsistency:

- TASK-040/TASK-045 made the *stall* loud in every profile because the release alternative was an unbounded retry spin with zero diagnostic output — nothing else could tell anyone anything.
- This violation already has a release-visible channel: `record_dropped_for_space(framed.len())` bumps `dropped_full`/`bytes_dropped`, surfaced by `console::snapshot()` and printed in STATUS. The record is lost either way; the board keeps playing music.
- Crashing here would reset a performer's pedal mid-song because a bulk dump chunk was refused — a release-semantics change this ticket explicitly disclaims ("the shipping release image is unaffected"). If the owner later wants it loud everywhere, that is a one-line change with its own decision, not a silent side effect of a debug-safety fix.
- Therefore: compute unconditionally (no `#[cfg(debug_assertions)]` anywhere — the repo has zero uses and a profile-split struct definition fragments `commit_records` for a few saved instructions), crash only in debug.

## 4. Step 3 — `frame.rs` (and `dump.rs`): make the encoder's checks compile-time

1. Name the prefix geometry that is currently magic numbers, beside `PREFIX_LEN`/`T_MS_WRAP`:
   ```rust
   const SEQ_AT: usize = 3;
   const SEQ_DIGITS: usize = 8;
   const T_MS_AT: usize = SEQ_AT + SEQ_DIGITS + 1;      // 12
   const T_MS_DIGITS: usize = 8;
   const CRC_DIGITS: usize = 4;
   const _: () = assert!(T_MS_AT + T_MS_DIGITS + 1 == PREFIX_LEN);   // fields tile the prefix
   const _: () = assert!(TRAILER_LEN == 1 + CRC_DIGITS + 2);         // '*' + crc hex + CR + LF
   ```
   `encode` then writes `out[SEQ_AT..SEQ_AT + SEQ_DIGITS]`, `out[SEQ_AT + SEQ_DIGITS] = b' '`, `out[T_MS_AT..T_MS_AT + T_MS_DIGITS]`, `out[T_MS_AT + T_MS_DIGITS] = b' '`, `out[trailer + 1..trailer + 1 + CRC_DIGITS]`. Wire bytes must not change by even one position — the 71 integration tests pin frames byte-for-byte and will catch any slip.
2. Give `frame.rs` the one owner of the width rule, callable in const context from both modules:
   ```rust
   pub(crate) const fn check_hex_width(digits: usize) {
       assert!(digits <= 8, "hex field wider than a u32");
   }
   pub(crate) const fn check_decimal_fits(max_value: u32, digits: usize) {
       assert!(max_value < 10u32.pow(digits as u32), "value cannot fit the decimal field");
   }
   ```
   Document that a bad width is now a **compile error** (const eval refuses `10u32.pow(10)` too), which is strictly better than a debug-only panic on the record path.
3. Apply them where the widths are declared: `check_hex_width(SEQ_DIGITS)`, `check_hex_width(CRC_DIGITS)`, `check_decimal_fits(T_MS_WRAP - 1, T_MS_DIGITS)` in frame.rs; `check_hex_width(BLK_HEX_DIGITS)`, `check_hex_width(COUNT_HEX_DIGITS)`, `check_hex_width(CRC_HEX_DIGITS)`, `check_decimal_fits(MAX_BLOCK_BYTES as u32, BYTES_DEC_DIGITS)` in dump.rs beside the existing width consts (dump.rs is not on the lock path today, but removing the runtime asserts takes its protection away too — restoring it at compile time is not scope creep, it is not losing coverage).
4. Delete `write_hex`'s `debug_assert!` and `write_decimal`'s `fits` binding + `debug_assert!`. Rewrite their doc paragraphs: the precondition is enforced at every width declaration by `check_*`, the renderers deliberately carry no runtime check because they are reached from inside the record lock, and on a hypothetical mismatch `write_hex` would shift past the width of `u32` (debug panic) and `write_decimal` would drop leading digits. Reject the const-generic `&mut [u8; N]` alternative in one sentence: getting an array reference out of `out[3..11]` needs `try_into().unwrap()`, which reintroduces an in-lock panic at the boundary.

Release behaviour: identical bytes, and one fewer arithmetic operation on the encode path. Debug behaviour: identical, because neither assert could fire.

## 5. Step 4 — host tests per converted site (default features, so CI runs them)

Add a `#[cfg(test)] mod tests` to `lib.rs` (the crate has none today) and extend `frame.rs`'s existing suite. Each test's doc comment states what is asserted and why the crash itself is unreachable on host, citing the undefined-symbol evidence in §1 — the way frame.rs:1305-1316 already does.

1. `refusal_after_admission` truth table, four rows: `(admitted=false, RefusedForSpace)` → `Violation::NONE`; `(admitted=true, RefusedForSpace)` → `Violation::noted(_, framed_len)` (assert equality against that value, and assert the carried message string and byte count); `(admitted=true, Committed)` → `NONE`; `(admitted=true, Stalled)` → `NONE`, because that case belongs to the stall panic. These are the "reported outcome, not the crash" assertions the acceptance criteria ask for.
2. The §3a equivalence pin: `encode(level, seq, now, &[b'x'; MAX_BODY], …).truncated == false` and with `MAX_BODY + 1` bytes `== true`, i.e. the deleted assert's predicate is `body.len() > MAX_BODY` and the guard excludes it. Put it in `frame.rs`'s suite where the other truncation tests live (:1034 already asserts the `> MAX_BODY` direction — reuse its helpers).
3. Width-pinning tests are unnecessary as tests (they are compile-time), but add one behavioural test that `write_decimal` emits exactly `T_MS_DIGITS` digits for `T_MS_WRAP - 1` and for `0`, so the renderer's contract is still covered after the assert goes away.
4. `record_lock_invariants.rs` itself is the fifth "test": it is what keeps all of this from regressing.

## 6. Step 5 — docs and the one enabling lint

### 6a. `usb.rs:421` → `msg.len().is_multiple_of(MAX_PACKET_SIZE as usize)`

Pre-existing, invisible to every current gate, and the only thing standing between this repo and a `log-usb` clippy gate. Mention it explicitly in the commit message as an enabling change, not a drive-by. If applying it surfaces further `log-usb`-only violations, fix each mechanically; stop and report if any needs a judgement call.

### 6b. Doc corrections

1. `commit_records`' doc (lib.rs:289-296) — currently names only `WriteOutcome::Stalled` as the thing callers report out of the closure. Generalize: callers report *any* contract break as a value; this function is the only place that crashes, and it crashes outside the lock. Keep the abort/PRIMASK paragraph.
2. `try_emit_dump`'s doc bullet that still reads as "assertions live here" — replace with the new shape (violations are computed under the lock, crashed after it; the counters are taken under the lock).
3. `write_hex` / `write_decimal` docs — per §4.4.
4. `usb::emit_panic_record` (usb.rs:337-345) — its claim that record-path callers crash only outside the record lock becomes fully true once these two asserts are gone; tighten the wording to name that mechanism and drop the trailing hedge that points at this ticket as an outstanding hazard.
5. Link direction rule (from TASK-046): links may point **into** ungated items, never from ungated docs into `log-usb`-gated ones. Document the new types in terms of `frame::WriteOutcome` and prose-name `emit`/`try_emit_dump` — `[`emit`]` is already one of TASK-043.02's 15 known rustdoc warnings, do not add another.

## 7. Verification, in this order

1. `nix develop -c cargo test -p asperitas-logging --test record_lock_invariants` → green (was red in §2c)
2. `nix develop -c cargo build -p asperitas-logging --features log-usb --all-targets` → compiles (fast check that the gated code still typechecks)
3. `nix develop -c cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings` → green (fails at HEAD; see §6a)
4. `nix develop -c cargo fmt --all --check`
5. `nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings`
6. `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` and the same with `--features asperitas-pod/pod-hw`
7. `nix develop -c cargo test -p asperitas-logging` then `nix develop -c cargo test --workspace`
8. `nix develop -c cargo doc -p asperitas-logging --no-deps` under default features and with `--features boot-led,log-usb,log-defmt` → no new warnings (pre-empting TASK-043.02's gate, as TASK-046 did)
9. `cd firmware && nix develop -c cargo build --release --features seed3`, then `cd firmware && nix develop -c make clippy FEATURES=seed3` (the only warnings-fatal build of the device binary)
10. `git --no-ext-diff diff --stat firmware/Cargo.lock` and `git --no-ext-diff diff --stat Cargo.lock` → both empty
11. Add the §3 gate line from step 3 to **both** `.github/workflows/ci.yml` (next to the existing clippy steps) and `lefthook.yml` (pre-push). Precedent: TASK-043's plan puts its doc gate in both files. Note in Implementation Notes that TASK-043 is `Blocked` and also plans to touch these two files.

## 8. Residuals — name them, do not fix them here

- **A caller-supplied `fill` closure runs under the lock** (lib.rs:343) and arbitrary `Display` impls may panic; that masks PRIMASK just as badly and no machine check can see it. Out of this ticket's letter; one sentence in `emit`'s doc saying so is the right size, and if the owner wants it closed, it needs formatting hoisted out of the lock — its own ticket.
- **TASK-046 already bounded the damage** this ticket's failure mode caused: `emit_blocking` now stops on CPU cycles (480 MHz × 3 s) plus a 20 M-poll ceiling instead of the time-driver clock whose ISR a mask freezes. Before that fix a debug assert meant an unbounded hang; after it, a bounded one. Say so in one sentence — it is why this ticket is Low priority rather than High.
- **`dump::audio_record` / `audend_record` call `frame::encode`** and therefore inherit the width rules, but have no device caller yet (TASK-038.03 wires them). When they run inside a `RECORD_BUFS` closure, the tripwire in §2b.5 is what forces a fresh look.
- `console_dump.rs:2039-2043` and `lib.rs:255-257` describe the host link failure correctly for code that *touches* the lock; §1 records the nuance (feature-enabled test binaries still link when nothing references the symbols). Don't rewrite them, and don't let anyone conclude the entry point is host-callable.

## 9. In the Final Summary

State explicitly: (a) the two remaining in-lock `debug_assert!`s in `try_emit_dump` are gone — one deleted as a provable tautology with a host test pinning the equivalence, one converted to a value reported out of the closure beside `frame::WriteOutcome` — an opaque carrier, deliberately not a fourth `WriteOutcome` variant and not a parallel outcome type — and crashed on in `commit_records` after the lock releases; (b) why the check there stays debug-profile while TASK-045's stall panic is unconditional (this one already reaches the wire as a counted drop; crashing would reset a live pedal for a refused dump chunk); (c) the encoder's two asserts became compile-time checks through one shared const helper, widths are now named constants that tile `PREFIX_LEN`/`TRAILER_LEN`, and deleting `write_decimal`'s guard also removed a latent all-profile `u32::pow` overflow; (d) the machine check is now a committed test that CI runs, that it scans `commit_records(` closures rather than `RECORD_BUFS.lock` (the funnel moved the bodies and the old grep would have passed vacuously), that it was written first and observed failing on exactly the four known sites, and that it cannot be satisfied by deleting the crashes; (e) that `clippy --features log-usb --lib -D warnings` is now green and wired into both gate files, and that it caught one pre-existing violation at usb.rs:421; (f) that release behaviour is unchanged on both paths (same counters, same return values, same bytes) and `Cargo.lock`/`firmware/Cargo.lock` are untouched; (g) that no part of this ticket was markable `HUMAN:` and why.
<!-- SECTION:PLAN:END -->
